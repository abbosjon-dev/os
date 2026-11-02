//! Fayl mazmuni: bloklar röyxati. Har bir blok — diskda (ZaminFS), keşda (xotirada)
//! yoki ikkalasida. Butun fayl xotiraga yuklanmaydi: kerakli bloklar diskdan öqiladi,
//! keş töla bölsa toza bloklar keşdan çiqariladi.

use super::FsError;
use super::zaminfs::{self, BLOCK_DATA, BlockRef};
use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;

pub const CHUNK: usize = BLOCK_DATA;

pub struct Chunk {
    /// Diskdagi oxirgi saqlangan nusxa
    pub disk: Option<BlockRef>,
    /// Xotiradagi nusxa (`CHUNK` bayt)
    pub cache: Option<Box<[u8]>>,
    /// Keşdagi nusxa diskdagidan farq qiladi — saqlaş kerak
    pub dirty: bool,
}

impl Chunk {
    fn hole() -> Chunk {
        Chunk { disk: None, cache: None, dirty: false }
    }
}

pub struct FileData {
    pub size: usize,
    pub chunks: Vec<Chunk>,
}

impl FileData {
    pub fn new() -> FileData {
        FileData { size: 0, chunks: Vec::new() }
    }

    /// Faqat xotirada yaşaydigan fayl (masalan, yadro bilan kelgan dasturlar).
    pub fn from_bytes(bytes: &[u8]) -> FileData {
        let chunks = bytes
            .chunks(CHUNK)
            .map(|c| {
                let mut b = vec![0u8; CHUNK].into_boxed_slice();
                b[..c.len()].copy_from_slice(c);
                Chunk { disk: None, cache: Some(b), dirty: false }
            })
            .collect();
        FileData { size: bytes.len(), chunks }
    }

    /// Bloklar keşda emasmi — diskdan öqib keşga qöyadi.
    fn load(&mut self, i: usize) -> Result<&mut Box<[u8]>, FsError> {
        let c = &mut self.chunks[i];
        if c.cache.is_none() {
            let data = match c.disk {
                Some(r) => zaminfs::read_data(r).map_err(|e| match e {
                    zaminfs::ZfsError::Corrupt => FsError::Corrupt,
                    _ => FsError::Io,
                })?,
                None => vec![0u8; CHUNK], // teşik
            };
            c.cache = Some(data.into_boxed_slice());
        }
        Ok(c.cache.as_mut().unwrap())
    }

    pub fn read(&mut self, offset: usize, buf: &mut [u8]) -> Result<usize, FsError> {
        let start = offset.min(self.size);
        let n = buf.len().min(self.size - start);
        let mut done = 0;
        while done < n {
            let pos = start + done;
            let (i, off) = (pos / CHUNK, pos % CHUNK);
            let take = (CHUNK - off).min(n - done);
            let block = self.load(i)?;
            buf[done..done + take].copy_from_slice(&block[off..off + take]);
            done += take;
        }
        Ok(n)
    }

    pub fn write(&mut self, offset: usize, data: &[u8]) -> Result<usize, FsError> {
        let end = offset.checked_add(data.len()).ok_or(FsError::TooBig)?;
        let need = end.div_ceil(CHUNK);
        while self.chunks.len() < need {
            self.chunks.push(Chunk::hole());
        }
        let mut done = 0;
        while done < data.len() {
            let pos = offset + done;
            let (i, off) = (pos / CHUNK, pos % CHUNK);
            let take = (CHUNK - off).min(data.len() - done);
            let block = self.load(i)?;
            block[off..off + take].copy_from_slice(&data[done..done + take]);
            self.chunks[i].dirty = true;
            done += take;
        }
        self.size = self.size.max(end);
        Ok(data.len())
    }

    /// Faylni böşatadi; diskdagi bloklar keyingi saqlaşdan keyin böşatiladi.
    pub fn truncate(&mut self) {
        self.release_all();
        self.chunks.clear();
        self.size = 0;
    }

    pub fn release_all(&mut self) {
        for c in &self.chunks {
            if let Some(r) = c.disk {
                zaminfs::release(r.block);
            }
        }
    }

    pub fn read_all(&mut self) -> Result<Vec<u8>, FsError> {
        let mut out = vec![0u8; self.size];
        self.read(0, &mut out)?;
        Ok(out)
    }

    /// Keşdagi toza (diskda nusxasi bor) bloklarni çiqaradi. Natija: çiqarilganlar soni.
    pub fn evict_clean(&mut self) -> usize {
        let mut n = 0;
        for c in &mut self.chunks {
            if c.cache.is_some() && !c.dirty && c.disk.is_some() {
                c.cache = None;
                n += 1;
            }
        }
        n
    }

    pub fn cached_clean(&self) -> usize {
        self.chunks.iter().filter(|c| c.cache.is_some() && !c.dirty && c.disk.is_some()).count()
    }
}
