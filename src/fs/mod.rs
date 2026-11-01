//! Virtual fayl tizimi (VFS) va xotiradagi fayl tizimi (ramfs).
//!
//! Hozirça barça fayllar RAM da saqlanadi (qayta yuklanganda yöqoladi).
//! Disk drayveri paydo bölgaç, şu interfeys orqali diskdagi fayl tizimi ulanadi.
//!
//! Boşlangʼiç tuzilma:
//!   /dasturlar   — yadro bilan kelgan foydalanuvçi dasturlari (ELF)
//!   /tizim       — tizim haqida maʼlumot
//!   /uy          — foydalanuvçi fayllari

use crate::abi::*;
use crate::sync::SpinLock;
use alloc::string::String;
use alloc::vec::Vec;

pub type NodeId = usize;

const ROOT: NodeId = 0;
const MAX_FILE: usize = 16 << 20; // 16 MiB
const MAX_NAME: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsError {
    NotFound,
    Exists,
    NotDir,
    IsDir,
    NotEmpty,
    Invalid,
    TooBig,
    Denied,
}

impl FsError {
    pub fn code(self) -> i64 {
        match self {
            FsError::NotFound => ERR_NOT_FOUND,
            FsError::Exists => ERR_EXISTS,
            FsError::NotDir => ERR_NOT_DIR,
            FsError::IsDir => ERR_IS_DIR,
            FsError::NotEmpty => ERR_NOT_EMPTY,
            FsError::Invalid => ERR_INVALID,
            FsError::TooBig => ERR_TOO_BIG,
            FsError::Denied => ERR_DENIED,
        }
    }
}

enum Data {
    File(Vec<u8>),
    Dir(Vec<NodeId>),
}

struct Node {
    name: String,
    parent: NodeId,
    data: Data,
    /// Tizim fayllari (öçirib / özgartirib bölmaydi)
    readonly: bool,
}

pub struct Entry {
    pub name: String,
    pub is_dir: bool,
    pub size: usize,
}

struct Vfs {
    /// Tugunlar; öçirilgan tugun örnida `None` (raqamlar qayta işlatilmaydi,
    /// şuning uçun eski dastak heç qaçon boşqa faylga tegmaydi).
    nodes: Vec<Option<Node>>,
}

static VFS: SpinLock<Vfs> = SpinLock::new(Vfs { nodes: Vec::new() });

impl Vfs {
    fn node(&self, id: NodeId) -> Result<&Node, FsError> {
        self.nodes.get(id).and_then(|n| n.as_ref()).ok_or(FsError::NotFound)
    }

    fn node_mut(&mut self, id: NodeId) -> Result<&mut Node, FsError> {
        self.nodes.get_mut(id).and_then(|n| n.as_mut()).ok_or(FsError::NotFound)
    }

    fn child(&self, dir: NodeId, name: &str) -> Result<NodeId, FsError> {
        match &self.node(dir)?.data {
            Data::Dir(children) => children
                .iter()
                .copied()
                .find(|&c| self.node(c).is_ok_and(|n| n.name == name))
                .ok_or(FsError::NotFound),
            Data::File(_) => Err(FsError::NotDir),
        }
    }

    fn lookup(&self, path: &str) -> Result<NodeId, FsError> {
        let mut cur = ROOT;
        for part in path.split('/').filter(|p| !p.is_empty() && *p != ".") {
            cur = if part == ".." { self.node(cur)?.parent } else { self.child(cur, part)? };
        }
        Ok(cur)
    }

    /// Yöldan (ota papka, oxirgi nom) ajratib oladi.
    fn split<'a>(&self, path: &'a str) -> Result<(NodeId, &'a str), FsError> {
        let trimmed = path.trim_end_matches('/');
        let (dir, name) = match trimmed.rfind('/') {
            Some(i) => (&trimmed[..i], &trimmed[i + 1..]),
            None => ("", trimmed),
        };
        if name.is_empty() || name == "." || name == ".." || name.len() > MAX_NAME {
            return Err(FsError::Invalid);
        }
        let parent = self.lookup(dir)?;
        match self.node(parent)?.data {
            Data::Dir(_) => Ok((parent, name)),
            Data::File(_) => Err(FsError::NotDir),
        }
    }

    fn add(&mut self, parent: NodeId, name: &str, data: Data, readonly: bool) -> Result<NodeId, FsError> {
        if self.child(parent, name).is_ok() {
            return Err(FsError::Exists);
        }
        let id = self.nodes.len();
        self.nodes.push(Some(Node { name: String::from(name), parent, data, readonly }));
        if let Data::Dir(children) = &mut self.node_mut(parent)?.data {
            children.push(id);
        }
        Ok(id)
    }
}

pub fn init(programs: &[(&str, &[u8])]) {
    let mut v = VFS.lock();
    v.nodes.push(Some(Node { name: String::new(), parent: ROOT, data: Data::Dir(Vec::new()), readonly: true }));
    let progs = v.add(ROOT, "dasturlar", Data::Dir(Vec::new()), true).unwrap();
    let sys = v.add(ROOT, "tizim", Data::Dir(Vec::new()), true).unwrap();
    v.add(ROOT, "uy", Data::Dir(Vec::new()), false).unwrap();
    for (name, image) in programs {
        v.add(progs, name, Data::File(image.to_vec()), true).unwrap();
    }
    let info = [
        ("nomi", String::from("Zamin OS\n")),
        ("versiya", alloc::format!("{}\n", env!("CARGO_PKG_VERSION"))),
        ("dasturçi", String::from("Abbosjon Dev <info@zamin.app>\nhttps://zamin.app\n")),
    ];
    for (name, text) in info {
        v.add(sys, name, Data::File(text.into_bytes()), true).unwrap();
    }
}

/// Faylni oçadi (kerak bölsa yaratadi / tozalaydi) va uning raqamini qaytaradi.
pub fn open(path: &str, flags: u64) -> Result<NodeId, FsError> {
    let mut v = VFS.lock();
    let id = match v.lookup(path) {
        Ok(id) => id,
        Err(FsError::NotFound) if flags & OPEN_CREATE != 0 => {
            let (parent, name) = v.split(path)?;
            if v.node(parent)?.readonly {
                return Err(FsError::Denied);
            }
            v.add(parent, name, Data::File(Vec::new()), false)?
        }
        Err(e) => return Err(e),
    };
    let node = v.node_mut(id)?;
    match &mut node.data {
        Data::Dir(_) => return Err(FsError::IsDir),
        Data::File(d) if flags & OPEN_TRUNCATE != 0 => {
            if node.readonly {
                return Err(FsError::Denied);
            }
            d.clear();
        }
        Data::File(_) => {}
    }
    Ok(id)
}

pub fn read(id: NodeId, offset: usize, buf: &mut [u8]) -> Result<usize, FsError> {
    match &VFS.lock().node(id)?.data {
        Data::File(d) => {
            let start = offset.min(d.len());
            let n = buf.len().min(d.len() - start);
            buf[..n].copy_from_slice(&d[start..start + n]);
            Ok(n)
        }
        Data::Dir(_) => Err(FsError::IsDir),
    }
}

/// Butun faylni öqiydi.
pub fn read_all(path: &str) -> Result<Vec<u8>, FsError> {
    let v = VFS.lock();
    match &v.node(v.lookup(path)?)?.data {
        Data::File(d) => Ok(d.clone()),
        Data::Dir(_) => Err(FsError::IsDir),
    }
}

/// `offset` dan boşlab yozadi (`None` — fayl oxiriga). Natija: yozilgan baytlar.
pub fn write(id: NodeId, offset: Option<usize>, data: &[u8]) -> Result<usize, FsError> {
    let mut v = VFS.lock();
    let node = v.node_mut(id)?;
    if node.readonly {
        return Err(FsError::Denied);
    }
    match &mut node.data {
        Data::File(d) => {
            let off = offset.unwrap_or(d.len());
            let end = off.checked_add(data.len()).ok_or(FsError::TooBig)?;
            if end > MAX_FILE {
                return Err(FsError::TooBig);
            }
            if end > d.len() {
                d.resize(end, 0);
            }
            d[off..end].copy_from_slice(data);
            Ok(data.len())
        }
        Data::Dir(_) => Err(FsError::IsDir),
    }
}

pub fn mkdir(path: &str) -> Result<NodeId, FsError> {
    let mut v = VFS.lock();
    let (parent, name) = v.split(path)?;
    if v.node(parent)?.readonly {
        return Err(FsError::Denied);
    }
    v.add(parent, name, Data::Dir(Vec::new()), false)
}

pub fn remove(path: &str) -> Result<(), FsError> {
    let mut v = VFS.lock();
    let id = v.lookup(path)?;
    let node = v.node(id)?;
    if id == ROOT || node.readonly {
        return Err(FsError::Denied);
    }
    if let Data::Dir(c) = &node.data {
        if !c.is_empty() {
            return Err(FsError::NotEmpty);
        }
    }
    let parent = node.parent;
    if let Data::Dir(children) = &mut v.node_mut(parent)?.data {
        children.retain(|&c| c != id);
    }
    v.nodes[id] = None;
    Ok(())
}

pub fn list(path: &str) -> Result<Vec<Entry>, FsError> {
    let v = VFS.lock();
    let id = v.lookup(path)?;
    match &v.node(id)?.data {
        Data::Dir(children) => {
            let mut out: Vec<Entry> = children
                .iter()
                .filter_map(|&c| v.node(c).ok())
                .map(|n| match &n.data {
                    Data::File(d) => Entry { name: n.name.clone(), is_dir: false, size: d.len() },
                    Data::Dir(c) => Entry { name: n.name.clone(), is_dir: true, size: c.len() },
                })
                .collect();
            out.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(a.name.cmp(&b.name)));
            Ok(out)
        }
        Data::File(_) => Err(FsError::NotDir),
    }
}

/// (fayllar soni, papkalar soni, jami bayt)
pub fn stats() -> (usize, usize, usize) {
    let v = VFS.lock();
    let (mut files, mut dirs, mut bytes) = (0, 0, 0);
    for n in v.nodes.iter().flatten() {
        match &n.data {
            Data::File(d) => {
                files += 1;
                bytes += d.len();
            }
            Data::Dir(_) => dirs += 1,
        }
    }
    (files, dirs, bytes)
}
