//! Fayllar bilan işlaş.

use crate::abi::*;
use crate::{Result, String, Vec, check, syscall};

/// Oçiq fayl. Yopiş — avtomatik (`Drop`).
pub struct File(u64);

impl File {
    /// Dastak raqami (masalan, bola jarayonning stdout i sifatida beriş uçun).
    pub fn handle(&self) -> u64 {
        self.0
    }

    pub fn open(path: &str, flags: u64) -> Result<File> {
        check(unsafe { syscall(SYS_OPEN, path.as_ptr() as u64, path.len() as u64, flags, 0) }).map(File)
    }

    /// Yangi fayl yaratadi (bor bölsa — tozalaydi).
    pub fn create(path: &str) -> Result<File> {
        File::open(path, OPEN_CREATE | OPEN_TRUNCATE)
    }

    pub fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        check(unsafe { syscall(SYS_READ, self.0, buf.as_mut_ptr() as u64, buf.len() as u64, 0) }).map(|n| n as usize)
    }

    pub fn write(&mut self, data: &[u8]) -> Result<usize> {
        check(unsafe { syscall(SYS_FWRITE, self.0, data.as_ptr() as u64, data.len() as u64, 0) }).map(|n| n as usize)
    }

    pub fn read_to_end(&mut self) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        let mut buf = [0u8; 512];
        loop {
            let n = self.read(&mut buf)?;
            if n == 0 {
                return Ok(out);
            }
            out.extend_from_slice(&buf[..n]);
        }
    }
}

impl Drop for File {
    fn drop(&mut self) {
        unsafe { syscall(SYS_CLOSE, self.0, 0, 0, 0) };
    }
}

pub fn read(path: &str) -> Result<Vec<u8>> {
    File::open(path, 0)?.read_to_end()
}

pub fn read_to_string(path: &str) -> Result<String> {
    Ok(String::from_utf8(read(path)?).unwrap_or_default())
}

pub fn write(path: &str, data: &[u8]) -> Result<()> {
    File::create(path)?.write(data).map(|_| ())
}

/// Papka yozuvi.
pub struct Entry {
    pub name: String,
    pub is_dir: bool,
    /// Fayl — baytlar, papka — içidagi yozuvlar soni
    pub size: u64,
}

/// Papka tarkibi, batafsil.
pub fn entries(path: &str) -> Result<Vec<Entry>> {
    let mut buf = crate::vec![0u8; 16384];
    let n = check(unsafe {
        syscall(SYS_READDIR, path.as_ptr() as u64, path.len() as u64, buf.as_mut_ptr() as u64, buf.len() as u64)
    })? as usize;
    let text = core::str::from_utf8(&buf[..n]).unwrap_or("");
    Ok(text
        .lines()
        .map(|l| {
            let (name, size) = l.split_once('\t').unwrap_or((l, "0"));
            let is_dir = name.ends_with('/');
            Entry {
                name: String::from(name.trim_end_matches('/')),
                is_dir,
                size: size.parse().unwrap_or(0),
            }
        })
        .collect())
}

/// Papka tarkibi (papkalar nomi oxirida "/").
pub fn list(path: &str) -> Result<Vec<String>> {
    Ok(entries(path)?
        .into_iter()
        .map(|e| if e.is_dir { e.name + "/" } else { e.name })
        .collect())
}

pub fn mkdir(path: &str) -> Result<()> {
    check(unsafe { syscall(SYS_MKDIR, path.as_ptr() as u64, path.len() as u64, 0, 0) }).map(|_| ())
}

pub fn remove(path: &str) -> Result<()> {
    check(unsafe { syscall(SYS_REMOVE, path.as_ptr() as u64, path.len() as u64, 0, 0) }).map(|_| ())
}
