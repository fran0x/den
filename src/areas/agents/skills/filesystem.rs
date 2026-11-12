//! Bounded symlink resolution and descriptor-relative reads. Never open FIFOs/devices.
use std::{
    collections::VecDeque,
    ffi::OsString,
    fs, io,
    path::{Component, Path, PathBuf},
};

pub fn resolve(path: &Path) -> io::Result<PathBuf> {
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    let mut pending: VecDeque<OsString> = path
        .components()
        .map(|c| c.as_os_str().to_owned())
        .collect();
    let mut resolved = PathBuf::new();
    let mut links = 0;
    let mut work = 0;
    while let Some(part) = pending.pop_front() {
        work += 1;
        if work > 2048 {
            return Err(io::Error::other("path component limit"));
        }
        match Path::new(&part).components().next() {
            Some(Component::RootDir) => resolved = PathBuf::from("/"),
            Some(Component::CurDir) => {}
            Some(Component::ParentDir) => {
                resolved.pop();
            }
            Some(Component::Normal(_)) => {
                resolved.push(&part);
                if fs::symlink_metadata(&resolved)?.file_type().is_symlink() {
                    links += 1;
                    if links > 32 {
                        return Err(io::Error::other("symlink limit or cycle"));
                    }
                    let target = fs::read_link(&resolved)?;
                    resolved.pop();
                    for component in target.components().rev() {
                        pending.push_front(component.as_os_str().to_owned());
                    }
                }
            }
            _ => return Err(io::Error::other("unsupported path")),
        }
    }
    Ok(resolved)
}

#[cfg(unix)]
mod platform {
    use super::*;
    use std::{
        ffi::{CStr, CString, OsStr},
        fs::File,
        io::Read,
        os::{
            fd::{AsRawFd, FromRawFd},
            unix::ffi::OsStrExt,
        },
    };
    pub struct Directory(pub File);
    impl Directory {
        pub fn open(path: &Path) -> io::Result<Self> {
            if !path.is_absolute() {
                return Err(io::Error::other("absolute resolved root required"));
            }
            let mut directory = Self(File::open("/")?);
            for c in path.components() {
                if let Component::Normal(name) = c {
                    let file = directory.child(name)?;
                    if !file.metadata()?.is_dir() {
                        return Err(io::Error::other("expected directory"));
                    }
                    directory = Self(file);
                }
            }
            Ok(directory)
        }
        pub fn child(&self, name: &OsStr) -> io::Result<File> {
            if Path::new(name).components().count() != 1
                || !matches!(
                    Path::new(name).components().next(),
                    Some(Component::Normal(_))
                )
            {
                return Err(io::Error::other("invalid child name"));
            }
            let name =
                CString::new(name.as_bytes()).map_err(|_| io::Error::other("invalid filename"))?;
            // Inspect type without following links before opening special files.
            let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
            // SAFETY: valid fd, C string and writable stat allocation.
            if unsafe {
                libc::fstatat(
                    self.0.as_raw_fd(),
                    name.as_ptr(),
                    stat.as_mut_ptr(),
                    libc::AT_SYMLINK_NOFOLLOW,
                )
            } < 0
            {
                return Err(io::Error::last_os_error());
            }
            // SAFETY: fstatat returned success, so it fully initialised `stat`.
            let stat = unsafe { stat.assume_init() };
            if !matches!(stat.st_mode & libc::S_IFMT, libc::S_IFREG | libc::S_IFDIR) {
                return Err(io::Error::other("symlink or special file; not opened"));
            }
            // SAFETY: a live directory fd and NUL-terminated child name; O_NOFOLLOW
            // rejects symlink swaps, O_NONBLOCK prevents FIFO/device opens from hanging.
            let fd = unsafe {
                libc::openat(
                    self.0.as_raw_fd(),
                    name.as_ptr(),
                    libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK,
                )
            };
            if fd < 0 {
                Err(io::Error::last_os_error())
            } else {
                // SAFETY: openat returned a new owned descriptor.
                Ok(unsafe { File::from_raw_fd(fd) })
            }
        }
        pub fn relative(&self, path: &Path) -> io::Result<File> {
            let mut dir = Self(self.0.try_clone()?);
            let parts: Vec<_> = path.components().collect();
            for (i, c) in parts.iter().enumerate() {
                let Component::Normal(name) = c else {
                    return Err(io::Error::other("path escapes skill"));
                };
                let file = dir.child(name)?;
                if i + 1 == parts.len() {
                    return Ok(file);
                }
                if !file.metadata()?.is_dir() {
                    return Err(io::Error::other("expected directory"));
                }
                dir = Self(file);
            }
            Err(io::Error::other("empty path"))
        }
        pub fn names(&self, limit: usize) -> io::Result<(Vec<OsString>, bool)> {
            // SAFETY: dup creates an independent descriptor owned by fdopendir/closedir.
            let fd = unsafe { libc::dup(self.0.as_raw_fd()) };
            if fd < 0 {
                return Err(io::Error::last_os_error());
            }
            // SAFETY: `fd` is a valid descriptor we own; on success fdopendir takes it over.
            let pointer = unsafe { libc::fdopendir(fd) };
            if pointer.is_null() {
                // SAFETY: fdopendir failed, so `fd` is still ours and is closed exactly once.
                unsafe { libc::close(fd) };
                return Err(io::Error::last_os_error());
            }
            struct Dir(*mut libc::DIR);
            impl Drop for Dir {
                fn drop(&mut self) {
                    // SAFETY: the pointer came from a successful fdopendir and is closed
                    // only here, once, when the guard drops.
                    unsafe { libc::closedir(self.0) };
                }
            }
            let dir = Dir(pointer);
            let mut names = vec![];
            let mut limited = false;
            loop {
                // readdir reports errors only through errno, so clear it first.
                #[cfg(target_os = "macos")]
                // SAFETY: returns this thread's errno location; always valid.
                let errno = unsafe { libc::__error() };
                #[cfg(not(target_os = "macos"))]
                // SAFETY: returns this thread's errno location; always valid.
                let errno = unsafe { libc::__errno_location() };
                // SAFETY: `errno` points to this thread's errno, valid for writes.
                unsafe {
                    *errno = 0;
                }
                // SAFETY: the DIR stays open until `dir` drops after the loop.
                let entry = unsafe { libc::readdir(dir.0) };
                if entry.is_null() {
                    // SAFETY: as above, this thread's errno, valid for reads.
                    if unsafe { *errno } != 0 {
                        return Err(io::Error::last_os_error());
                    }
                    break;
                }
                // SAFETY: readdir returned a live entry whose d_name is NUL-terminated; the
                // bytes are copied below before the next readdir call reuses the buffer.
                let name = unsafe { CStr::from_ptr((*entry).d_name.as_ptr()) }.to_bytes();
                if name == b"." || name == b".." {
                    continue;
                }
                if names.len() >= limit {
                    limited = true;
                    break;
                }
                names.push(OsStr::from_bytes(name).to_owned());
            }
            names.sort();
            Ok((names, limited))
        }
    }
    pub fn text(mut file: File, limit: usize) -> io::Result<String> {
        let metadata = file.metadata()?;
        if !metadata.is_file() {
            return Err(io::Error::other("not a regular file"));
        }
        if metadata.len() > limit as u64 {
            return Err(io::Error::other("file size limit"));
        }
        let mut bytes = Vec::new();
        (&mut file).take(limit as u64 + 1).read_to_end(&mut bytes)?;
        if bytes.len() > limit {
            return Err(io::Error::other("file size limit"));
        }
        if bytes.contains(&0) {
            return Err(io::Error::other("binary file; text preview unavailable"));
        }
        let text = String::from_utf8(bytes)
            .map_err(|_| io::Error::other("not UTF-8; text preview unavailable"))?;
        if text.lines().count() > 20_000 {
            return Err(io::Error::other("20000-line display limit exceeded"));
        }
        Ok(text)
    }
}
#[cfg(unix)]
pub use platform::*;
