//! Virtual fayl tizimi (VFS) va xotiradagi fayl tizimi (ramfs).
//!
//! Hozirça barça fayllar RAM da saqlanadi (qayta yuklanganda yöqoladi).
//! Disk drayveri paydo bölgaç, şu interfeys orqali diskdagi fayl tizimi ulanadi.
//!
//! Boşlangʼiç tuzilma (nomlar tizim tiliga qarab):
//!   /dasturlar  (/программы, /programs)  — yadro bilan kelgan dasturlar (ELF)
//!   /tizim      (/система,   /system)    — tizim haqida maʼlumot
//!   /uy         (/дом,       /home)      — foydalanuvçilar papkalari
//!   /uy/<login>/Hujjatlar, Yuklamalar, Rasmlar, Musiqa
//!
//! Tizim papkalarining "roli" bor: ular joriy tilda körsatiladi, lekin uç tildagi
//! istalgan nomi bilan topiladi. Şuning uçun `/dasturlar/salom` dasturlarda hamişa işlaydi.
//! `~` — joriy foydalanuvçining uy papkasi.

use crate::abi::*;
use crate::i18n::Lang;
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

/// Tizim papkalari va fayllarining roli: nomi tilga qarab özgaradi.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Programs,
    System,
    Home,
    SysName,
    SysVersion,
    SysDeveloper,
    Documents,
    Downloads,
    Pictures,
    Music,
}

impl Role {
    /// [uz, ru, en]
    fn names(self) -> [&'static str; 3] {
        match self {
            Role::Programs => ["dasturlar", "программы", "programs"],
            Role::System => ["tizim", "система", "system"],
            Role::Home => ["uy", "дом", "home"],
            Role::SysName => ["nomi", "название", "name"],
            Role::SysVersion => ["versiya", "версия", "version"],
            Role::SysDeveloper => ["dasturçi", "разработчик", "developer"],
            Role::Documents => ["Hujjatlar", "Документы", "Documents"],
            Role::Downloads => ["Yuklamalar", "Загрузки", "Downloads"],
            Role::Pictures => ["Rasmlar", "Изображения", "Pictures"],
            Role::Music => ["Musiqa", "Музыка", "Music"],
        }
    }

    fn matches(self, name: &str) -> bool {
        self.names().contains(&name)
    }
}

const HOME_FOLDERS: [Role; 4] = [Role::Documents, Role::Downloads, Role::Pictures, Role::Music];

struct Node {
    name: String,
    parent: NodeId,
    data: Data,
    /// Tizim fayllari (öçirib / özgartirib bölmaydi)
    readonly: bool,
    role: Option<Role>,
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
    /// `~` — joriy foydalanuvçining uy papkasi
    home: Option<NodeId>,
}

static VFS: SpinLock<Vfs> = SpinLock::new(Vfs { nodes: Vec::new(), home: None });

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
                .find(|&c| self.node(c).is_ok_and(|n| n.name == name || n.role.is_some_and(|r| r.matches(name))))
                .ok_or(FsError::NotFound),
            Data::File(_) => Err(FsError::NotDir),
        }
    }

    fn lookup(&self, path: &str) -> Result<NodeId, FsError> {
        let (mut cur, rest) = match path.strip_prefix('~') {
            Some(rest) => (self.home.ok_or(FsError::NotFound)?, rest),
            None => (ROOT, path),
        };
        for part in rest.split('/').filter(|p| !p.is_empty() && *p != ".") {
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
        self.add_with_role(parent, name, data, readonly, None)
    }

    fn add_with_role(
        &mut self,
        parent: NodeId,
        name: &str,
        data: Data,
        readonly: bool,
        role: Option<Role>,
    ) -> Result<NodeId, FsError> {
        if self.child(parent, name).is_ok() {
            return Err(FsError::Exists);
        }
        let id = self.nodes.len();
        self.nodes.push(Some(Node { name: String::from(name), parent, data, readonly, role }));
        if let Data::Dir(children) = &mut self.node_mut(parent)?.data {
            children.push(id);
        }
        Ok(id)
    }
}

pub fn init(programs: &[(&str, &[u8])]) {
    let mut v = VFS.lock();
    let lang = crate::i18n::current() as usize;
    v.nodes.push(Some(Node {
        name: String::new(),
        parent: ROOT,
        data: Data::Dir(Vec::new()),
        readonly: true,
        role: None,
    }));
    let dir = |r: Role| (r.names()[lang], Data::Dir(Vec::new()), Some(r));
    let (n, d, r) = dir(Role::Programs);
    let progs = v.add_with_role(ROOT, n, d, true, r).unwrap();
    let (n, d, r) = dir(Role::System);
    let sys = v.add_with_role(ROOT, n, d, true, r).unwrap();
    let (n, d, r) = dir(Role::Home);
    v.add_with_role(ROOT, n, d, false, r).unwrap();
    for (name, image) in programs {
        v.add(progs, name, Data::File(image.to_vec()), true).unwrap();
    }
    let info = [
        (Role::SysName, String::from("Zamin OS\n")),
        (Role::SysVersion, alloc::format!("{}\n", env!("CARGO_PKG_VERSION"))),
        (Role::SysDeveloper, String::from("Abbosjon Dev <info@zamin.app>\nhttps://zamin.app\n")),
    ];
    for (role, text) in info {
        v.add_with_role(sys, role.names()[lang], Data::File(text.into_bytes()), true, Some(role)).unwrap();
    }
}

/// Tizim tili özgarganda tizim papkalari va fayllarini qayta nomlaydi.
pub fn apply_language(lang: Lang) {
    let mut v = VFS.lock();
    for n in v.nodes.iter_mut().flatten() {
        if let Some(r) = n.role {
            n.name = String::from(r.names()[lang as usize]);
        }
    }
}

/// Foydalanuvçining uy papkasini (va standart içki papkalarini) yaratadi hamda
/// `~` ni unga yönaltiradi. Allaqaçon bor bölsa — şunçaki yönaltiradi.
pub fn setup_home(username: &str) -> Result<String, FsError> {
    let mut v = VFS.lock();
    let lang = crate::i18n::current() as usize;
    let homes = v.lookup("/uy")?;
    let home = match v.child(homes, username) {
        Ok(id) => id,
        Err(_) => v.add(homes, username, Data::Dir(Vec::new()), false)?,
    };
    for r in HOME_FOLDERS {
        if v.child(home, r.names()[0]).is_err() {
            v.add_with_role(home, r.names()[lang], Data::Dir(Vec::new()), false, Some(r))?;
        }
    }
    v.home = Some(home);
    let homes_name = v.node(homes)?.name.clone();
    Ok(alloc::format!("/{}/{}", homes_name, username))
}

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
