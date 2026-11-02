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

pub mod fat;
pub mod file;
pub mod zaminfs;

use file::FileData;

use crate::abi::*;
use crate::i18n::Lang;
use core::sync::atomic::{AtomicBool, Ordering};
use crate::sync::Mutex;
use alloc::string::String;
use alloc::vec::Vec;

pub type NodeId = usize;

const ROOT: NodeId = 0;
const MAX_FILE: usize = 256 << 20; // 256 MiB
/// Keşdagi toza bloklar şundan oşsa — keşdan çiqariladi (~8 MiB)
const CACHE_LIMIT: usize = 2048;
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
    /// Diskdagi maʼlumot buzilgan (tekşiruvdan ötmadi)
    Corrupt,
    /// Disk xatosi
    Io,
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
            FsError::Corrupt => ERR_CORRUPT,
            FsError::Io => ERR_IO,
        }
    }
}

enum Data {
    File(FileData),
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
    Videos,
    Temp,
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
            Role::Videos => ["Videolar", "Видео", "Videos"],
            Role::Temp => ["vaqtinçalik", "временные", "tmp"],
        }
    }

    fn matches(self, name: &str) -> bool {
        self.names().contains(&name)
    }
}

const HOME_FOLDERS: [Role; 5] = [Role::Documents, Role::Downloads, Role::Pictures, Role::Music, Role::Videos];

struct Node {
    name: String,
    parent: NodeId,
    data: Data,
    /// Tizim fayllari (öçirib / özgartirib bölmaydi)
    readonly: bool,
    role: Option<Role>,
    /// Egasi (foydalanuvçi raqami): 0 — tizim
    owner: u32,
    /// Ruxsatlar (rwxrwxrwx, sakkizlik)
    mode: u16,
    /// Yaratilgan va özgartirilgan vaqt (Unix soniyalar; RTC bölmasa — 0)
    ctime: u64,
    mtime: u64,
}

pub struct Entry {
    pub name: String,
    pub is_dir: bool,
    pub size: usize,
    pub mode: u16,
    pub owner: u32,
    pub mtime: u64,
}

struct Vfs {
    /// Tugunlar; öçirilgan tugun örnida `None` (raqamlar qayta işlatilmaydi,
    /// şuning uçun eski dastak heç qaçon boşqa faylga tegmaydi).
    nodes: Vec<Option<Node>>,
    /// `~` — joriy foydalanuvçining uy papkasi
    home: Option<NodeId>,
    /// Amalni bajarayotgan foydalanuvçi (0 — yadro, tekşiruvsiz)
    uid: u32,
}

/// Ruxsat bitlari (egasi uçun 6 ga suriladi)
pub const PERM_R: u16 = 4;
pub const PERM_W: u16 = 2;
pub const PERM_X: u16 = 1;
/// Papkadagi faylni faqat uning egasi öçira oladi (umumiy papkalar uçun)
pub const PERM_STICKY: u16 = 0o1000;

/// Uxlaydigan qulf: katta fayllar bilan işlaş boşqa vazifalarni (va uzilişlarni) töxtatmaydi.
static VFS: Mutex<Vfs> = Mutex::new(Vfs { nodes: Vec::new(), home: None, uid: 0 });

/// VFS ni joriy jarayon egasi nomidan oladi (ruxsatlar tekşiriladi).
fn vfs_as_caller() -> crate::sync::MutexGuard<'static, Vfs> {
    let uid = crate::task::current_uid();
    let mut v = VFS.lock();
    v.uid = uid;
    v
}

/// VFS ni yadro nomidan oladi (içki amallar: tiklaş, uy papkasi yaratiş).
fn vfs_as_kernel() -> crate::sync::MutexGuard<'static, Vfs> {
    let mut v = VFS.lock();
    v.uid = 0;
    v
}

/// Foydalanuvçi fayllari özgardi — diskka saqlaş kerak.
static DIRTY: AtomicBool = AtomicBool::new(false);

fn mark_dirty() {
    DIRTY.store(true, Ordering::Release);
}

/// Özgariş bölganmi (va belgini tozalaydi).
pub fn take_dirty() -> bool {
    DIRTY.swap(false, Ordering::AcqRel)
}

pub fn set_dirty() {
    mark_dirty();
}

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

    /// Foydalanuvçining tugunga nisbatan huquqlari: egasi — yuqori uç bit, boşqalar — pastki.
    fn allowed(&self, id: NodeId, need: u16) -> bool {
        if self.uid == 0 {
            return true;
        }
        let Ok(n) = self.node(id) else { return false };
        let bits = if n.owner == self.uid { (n.mode >> 6) & 7 } else { n.mode & 7 };
        bits & need == need
    }

    fn need(&self, id: NodeId, need: u16) -> Result<(), FsError> {
        if self.allowed(id, need) { Ok(()) } else { Err(FsError::Denied) }
    }

    /// Yöl böyiça tugun. Papkadan ötiş uçun undagi "x" ruxsati kerak.
    fn lookup(&self, path: &str) -> Result<NodeId, FsError> {
        let (mut cur, rest) = match path.strip_prefix('~') {
            Some(rest) => (self.home.ok_or(FsError::NotFound)?, rest),
            None => (ROOT, path),
        };
        for part in rest.split('/').filter(|p| !p.is_empty() && *p != ".") {
            self.need(cur, PERM_X)?;
            cur = if part == ".." { self.node(cur)?.parent } else { self.child(cur, part)? };
        }
        Ok(cur)
    }

    /// `id` tuguni `ancestor` içidami (yoki özimi)?
    fn is_inside(&self, mut id: NodeId, ancestor: NodeId) -> bool {
        loop {
            if id == ancestor {
                return true;
            }
            if id == ROOT {
                return false;
            }
            match self.node(id) {
                Ok(n) => id = n.parent,
                Err(_) => return false,
            }
        }
    }

    fn in_temp(&self, mut id: NodeId) -> bool {
        while id != ROOT {
            let Ok(n) = self.node(id) else { return false };
            if n.role == Some(Role::Temp) {
                return true;
            }
            id = n.parent;
        }
        false
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
        let mode = if matches!(data, Data::Dir(_)) { 0o755 } else { 0o644 };
        let owner = if readonly { 0 } else { self.uid };
        let now = crate::timer::unix_time();
        self.nodes.push(Some(Node {
            name: String::from(name),
            parent,
            data,
            readonly,
            role,
            owner,
            mode,
            ctime: now,
            mtime: now,
        }));
        if let Data::Dir(children) = &mut self.node_mut(parent)?.data {
            children.push(id);
        }
        Ok(id)
    }
}

pub fn init(programs: &[(&str, &[u8])]) {
    let mut v = vfs_as_kernel();
    let lang = crate::i18n::current() as usize;
    v.nodes.push(Some(Node {
        name: String::new(),
        parent: ROOT,
        data: Data::Dir(Vec::new()),
        readonly: true,
        role: None,
        owner: 0,
        mode: 0o755,
        ctime: 0,
        mtime: 0,
    }));
    let dir = |r: Role| (r.names()[lang], Data::Dir(Vec::new()), Some(r));
    let (n, d, r) = dir(Role::Programs);
    let progs = v.add_with_role(ROOT, n, d, true, r).unwrap();
    let (n, d, r) = dir(Role::System);
    let sys = v.add_with_role(ROOT, n, d, true, r).unwrap();
    let (n, d, r) = dir(Role::Home);
    v.add_with_role(ROOT, n, d, false, r).unwrap();
    // Umumiy vaqtinçalik papka: hamma yozadi, faqat öz faylini öçiradi; diskka saqlanmaydi
    let (n, d, r) = dir(Role::Temp);
    let tmp = v.add_with_role(ROOT, n, d, false, r).unwrap();
    v.node_mut(tmp).unwrap().mode = 0o777 | PERM_STICKY;
    for (name, image) in programs {
        let id = v.add(progs, name, Data::File(FileData::from_bytes(image)), true).unwrap();
        v.node_mut(id).unwrap().mode = 0o755; // bajariladigan
    }
    let info = [
        (Role::SysName, String::from("Zamin OS\n")),
        (Role::SysVersion, alloc::format!("{}\n", env!("CARGO_PKG_VERSION"))),
        (Role::SysDeveloper, String::from("Abbosjon Dev <info@zamin.app>\nhttps://zamin.app\n")),
    ];
    for (role, text) in info {
        v.add_with_role(sys, role.names()[lang], Data::File(FileData::from_bytes(text.as_bytes())), true, Some(role)).unwrap();
    }
}

/// Tizim tili özgarganda tizim papkalari va fayllarini qayta nomlaydi.
pub fn apply_language(lang: Lang) {
    let mut v = vfs_as_kernel();
    for n in v.nodes.iter_mut().flatten() {
        if let Some(r) = n.role {
            n.name = String::from(r.names()[lang as usize]);
        }
    }
}

/// Foydalanuvçining uy papkasini (va standart içki papkalarini) yaratadi hamda
/// `~` ni unga yönaltiradi. Allaqaçon bor bölsa — şunçaki yönaltiradi.
/// Foydalanuvçi uy papkasi; egasi — şu foydalanuvçi.
pub fn setup_home(username: &str, uid: u32) -> Result<String, FsError> {
    let mut v = vfs_as_kernel();
    let lang = crate::i18n::current() as usize;
    let homes = v.lookup("/uy")?;
    let home = match v.child(homes, username) {
        Ok(id) => id,
        Err(_) => v.add(homes, username, Data::Dir(Vec::new()), false)?,
    };
    {
        let n = v.node_mut(home)?;
        n.owner = uid;
        n.mode = 0o700; // maxfiy: boşqa foydalanuvçilar kira olmaydi
    }
    for r in HOME_FOLDERS {
        if v.child(home, r.names()[0]).is_err() {
            let id = v.add_with_role(home, r.names()[lang], Data::Dir(Vec::new()), false, Some(r))?;
            v.node_mut(id)?.owner = uid;
        }
    }
    // Namuna fayllar — mos papkalarga (yoziş yöli orqali — diskka saqlanadi)
    let pics = v.child(home, Role::Pictures.names()[lang]).ok();
    let music = v.child(home, Role::Music.names()[lang]).ok();
    let videos = v.child(home, Role::Videos.names()[lang]).ok();
    let seeds: [(Option<NodeId>, &str, &[u8]); 6] = [
        (pics, "zamin.png", crate::image::SAMPLE_PNG),
        (pics, "zamin.svg", crate::image::SAMPLE_SVG),
        (pics, "zamin.gif", crate::image::SAMPLE_GIF),
        (music, "zamin.wav", crate::audio::SAMPLE_WAV),
        (music, "zamin.mp3", &crate::audio::SAMPLE_MP3),
        (videos, "zamin.avi", &crate::video::SAMPLE_AVI),
    ];
    for (dir, name, bytes) in seeds {
        if let Some(dir) = dir {
            if v.child(dir, name).is_err() {
                if let Ok(id) = v.add(dir, name, Data::File(FileData::new()), false) {
                    if let Data::File(d) = &mut v.node_mut(id)?.data {
                        let _ = d.write(0, bytes);
                    }
                    v.node_mut(id)?.owner = uid;
                    mark_dirty();
                }
            }
        }
    }
    // `~` faqat tizimga kirgan foydalanuvçining öz papkasi (boşqa hisob yaratilganda özgarmaydi)
    if uid == crate::accounts::current_uid() {
        v.home = Some(home);
    }
    let homes_name = v.node(homes)?.name.clone();
    Ok(alloc::format!("/{}/{}", homes_name, username))
}

pub fn open(path: &str, flags: u64) -> Result<NodeId, FsError> {
    let mut v = vfs_as_caller();
    let id = match v.lookup(path) {
        Ok(id) => id,
        Err(FsError::NotFound) if flags & OPEN_CREATE != 0 => {
            let (parent, name) = v.split(path)?;
            if v.node(parent)?.readonly {
                return Err(FsError::Denied);
            }
            v.need(parent, PERM_W)?; // papkada yangi fayl yaratiş
            mark_dirty();
            v.add(parent, name, Data::File(FileData::new()), false)?
        }
        Err(e) => return Err(e),
    };
    let wants_write = flags & (OPEN_CREATE | OPEN_TRUNCATE | OPEN_APPEND | OPEN_WRITE) != 0;
    v.need(id, if wants_write { PERM_R | PERM_W } else { PERM_R })?;
    let node = v.node_mut(id)?;
    match &mut node.data {
        Data::Dir(_) => return Err(FsError::IsDir),
        Data::File(d) if flags & OPEN_TRUNCATE != 0 => {
            if node.readonly {
                return Err(FsError::Denied);
            }
            d.truncate();
            node.mtime = crate::timer::unix_time();
            mark_dirty();
        }
        Data::File(_) => {}
    }
    Ok(id)
}

pub fn read(id: NodeId, offset: usize, buf: &mut [u8]) -> Result<usize, FsError> {
    let mut v = vfs_as_kernel();
    let n = match &mut v.node_mut(id)?.data {
        Data::File(d) => d.read(offset, buf)?,
        Data::Dir(_) => return Err(FsError::IsDir),
    };
    v.maybe_evict();
    Ok(n)
}

/// Butun faylni öqiydi.
pub fn read_all(path: &str) -> Result<Vec<u8>, FsError> {
    read_checked(path, PERM_R)
}

/// Dasturni işga tuşiriş uçun öqiydi: "x" (bajariş) ruxsati ham kerak.
pub fn read_program(path: &str) -> Result<Vec<u8>, FsError> {
    read_checked(path, PERM_R | PERM_X)
}

fn read_checked(path: &str, perm: u16) -> Result<Vec<u8>, FsError> {
    let mut v = vfs_as_caller();
    let id = v.lookup(path)?;
    v.need(id, perm)?;
    let r = match &mut v.node_mut(id)?.data {
        Data::File(d) => d.read_all(),
        Data::Dir(_) => Err(FsError::IsDir),
    };
    v.maybe_evict();
    r
}

/// `offset` dan boşlab yozadi (`None` — fayl oxiriga). Natija: yozilgan baytlar.
pub fn write(id: NodeId, offset: Option<usize>, data: &[u8]) -> Result<usize, FsError> {
    let mut v = vfs_as_kernel();
    let node = v.node_mut(id)?;
    if node.readonly {
        return Err(FsError::Denied);
    }
    node.mtime = crate::timer::unix_time();
    match &mut node.data {
        Data::File(d) => {
            let off = offset.unwrap_or(d.size);
            let end = off.checked_add(data.len()).ok_or(FsError::TooBig)?;
            if end > MAX_FILE {
                return Err(FsError::TooBig);
            }
            let n = d.write(off, data)?;
            mark_dirty();
            Ok(n)
        }
        Data::Dir(_) => Err(FsError::IsDir),
    }
}

pub fn mkdir(path: &str) -> Result<NodeId, FsError> {
    let mut v = vfs_as_caller();
    let (parent, name) = v.split(path)?;
    if v.node(parent)?.readonly {
        return Err(FsError::Denied);
    }
    v.need(parent, PERM_W)?;
    mark_dirty();
    v.add(parent, name, Data::Dir(Vec::new()), false)
}

/// Yöl bölgan tugun (yadro huquqi bilan): qafas papkasini aniqlaş uçun.
pub fn node_of(path: &str) -> Result<NodeId, FsError> {
    vfs_as_kernel().lookup(path)
}

/// Yöl `jail` papkasi içidami? Hali yöq fayl uçun — uning ota papkasi tekşiriladi.
/// Nomlar emas, tugunlar soliştiriladi: ".." yoki boşqa tildagi nom bilan qoçib bölmaydi.
pub fn path_inside(path: &str, jail: NodeId) -> bool {
    let v = vfs_as_kernel();
    let id = match v.lookup(path) {
        Ok(id) => id,
        Err(_) => match v.split(path) {
            Ok((parent, _)) => parent,
            Err(_) => return false,
        },
    };
    v.is_inside(id, jail)
}

/// Ruxsatlarni özgartiradi (faqat egasi yoki yadro).
pub fn chmod(path: &str, mode: u16) -> Result<(), FsError> {
    let mut v = vfs_as_caller();
    let id = v.lookup(path)?;
    let uid = v.uid;
    let n = v.node_mut(id)?;
    if n.readonly || (uid != 0 && n.owner != uid) {
        return Err(FsError::Denied);
    }
    n.mode = mode & 0o1777;
    mark_dirty();
    Ok(())
}

pub fn remove(path: &str) -> Result<(), FsError> {
    let mut v = vfs_as_caller();
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
    let (parent, owner) = (node.parent, node.owner);
    v.need(parent, PERM_W)?;
    // Umumiy papkada (sticky) faqat egasi öçira oladi
    let sticky = v.node(parent)?.mode & PERM_STICKY != 0;
    if sticky && v.uid != 0 && owner != v.uid {
        return Err(FsError::Denied);
    }
    if let Data::Dir(children) = &mut v.node_mut(parent)?.data {
        children.retain(|&c| c != id);
    }
    if let Some(Node { data: Data::File(d), .. }) = v.nodes[id].as_mut() {
        d.release_all(); // diskdagi bloklar keyingi saqlaşdan keyin böşatiladi
    }
    v.nodes[id] = None;
    mark_dirty();
    Ok(())
}

pub fn list(path: &str) -> Result<Vec<Entry>, FsError> {
    let v = vfs_as_caller();
    let id = v.lookup(path)?;
    if matches!(v.node(id)?.data, Data::Dir(_)) {
        v.need(id, PERM_R)?;
    }
    match &v.node(id)?.data {
        Data::Dir(children) => {
            let mut out: Vec<Entry> = children
                .iter()
                .filter_map(|&c| v.node(c).ok())
                .map(|n| {
                    let (is_dir, size) = match &n.data {
                        Data::File(d) => (false, d.size),
                        Data::Dir(c) => (true, c.len()),
                    };
                    Entry { name: n.name.clone(), is_dir, size, mode: n.mode, owner: n.owner, mtime: n.mtime }
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
    let v = vfs_as_kernel();
    let (mut files, mut dirs, mut bytes) = (0, 0, 0);
    for n in v.nodes.iter().flatten() {
        match &n.data {
            Data::File(d) => {
                files += 1;
                bytes += d.size;
            }
            Data::Dir(_) => dirs += 1,
        }
    }
    (files, dirs, bytes)
}

// --- ZaminFS 2: metadata va saqlaş ---
//
// Metadata formati: "ZFS2-META-1" va yozuvlar (ota papka hamişa bolasidan oldin):
//   [tur: 1 = papka, 2 = fayl] [yöl: u16 + baytlar] [egasi: u32] [ruxsat: u16]
//   [yaratilgan: u64] [özgartirilgan: u64]
//   fayl uçun: [hajm: u64] [bloklar soni: u32] va har bir blok: [raqam: u32] [nonce: 12]
// Rolli tugunlar yölda kanonik (özbekça) nom bilan — til özgarsa ham tiklanadi.

const META_MAGIC: &[u8] = b"ZFS2-META-1";
const REC_DIR: u8 = 1;
const REC_FILE: u8 = 2;

impl Vfs {
    fn canonical_path(&self, mut id: NodeId) -> String {
        let mut parts: Vec<&str> = Vec::new();
        while id != ROOT {
            let Ok(n) = self.node(id) else { break };
            parts.push(n.role.map_or(n.name.as_str(), |r| r.names()[0]));
            id = n.parent;
        }
        parts.reverse();
        alloc::format!("/{}", parts.join("/"))
    }

    fn persistent(&self, id: NodeId) -> bool {
        self.node(id).is_ok_and(|n| id != ROOT && !n.readonly && n.role != Some(Role::Home)) && !self.in_temp(id)
    }

    fn serialize_node(&self, id: NodeId, out: &mut Vec<u8>) {
        let Ok(n) = self.node(id) else { return };
        if self.persistent(id) {
            let path = self.canonical_path(id);
            out.push(if matches!(n.data, Data::Dir(_)) { REC_DIR } else { REC_FILE });
            out.extend_from_slice(&(path.len() as u16).to_le_bytes());
            out.extend_from_slice(path.as_bytes());
            out.extend_from_slice(&n.owner.to_le_bytes());
            out.extend_from_slice(&n.mode.to_le_bytes());
            out.extend_from_slice(&n.ctime.to_le_bytes());
            out.extend_from_slice(&n.mtime.to_le_bytes());
            if let Data::File(d) = &n.data {
                out.extend_from_slice(&(d.size as u64).to_le_bytes());
                out.extend_from_slice(&(d.chunks.len() as u32).to_le_bytes());
                for c in &d.chunks {
                    let r = c.disk.unwrap_or(zaminfs::BlockRef { block: zaminfs::NO_BLOCK, nonce: [0; 12] });
                    out.extend_from_slice(&r.block.to_le_bytes());
                    out.extend_from_slice(&r.nonce);
                }
            }
        }
        if let Data::Dir(children) = &n.data {
            for &c in children {
                self.serialize_node(c, out);
            }
        }
    }

    /// Keşdagi toza bloklar köpaysa — ularni çiqaradi (keyin kerak bölsa diskdan qayta öqiladi).
    fn maybe_evict(&mut self) {
        let cached: usize = self
            .nodes
            .iter()
            .flatten()
            .map(|n| if let Data::File(d) = &n.data { d.cached_clean() } else { 0 })
            .sum();
        if cached > CACHE_LIMIT {
            for n in self.nodes.iter_mut().flatten() {
                if let Data::File(d) = &mut n.data {
                    d.evict_clean();
                }
            }
        }
    }
}

/// Çiqişda: foydalanuvçi fayllari xotiradan olib taşlanadi (keyingi kirişda diskdan qayta öqiladi).
/// Ikkinçi diskdagi FAT fayl tizimini `/fat` ga ulaydi (faqat öqiş). Natija: fayllar soni.
pub fn mount_fat() -> usize {
    let Some(files) = fat::read_root() else { return 0 };
    let mut v = vfs_as_kernel();
    let fat_dir = match v.lookup("/fat") {
        Ok(id) => id,
        Err(_) => match v.add(ROOT, "fat", Data::Dir(Vec::new()), true) {
            Ok(id) => id,
            Err(_) => return 0,
        },
    };
    let mut n = 0;
    for (name, data) in files {
        let lower = name.to_ascii_lowercase();
        if v.child(fat_dir, &lower).is_err() {
            if let Ok(id) = v.add(fat_dir, &lower, Data::File(FileData::from_bytes(&data)), true) {
                v.node_mut(id).unwrap().mode = 0o444;
                n += 1;
            }
        }
    }
    n
}

pub fn forget_user_files() {
    let mut v = vfs_as_kernel();
    let ids: Vec<NodeId> = (0..v.nodes.len()).filter(|&id| v.persistent(id)).collect();
    for id in ids.into_iter().rev() {
        let parent = v.nodes[id].as_ref().map(|n| n.parent);
        if let Some(p) = parent {
            if let Ok(Node { data: Data::Dir(children), .. }) = v.node_mut(p) {
                children.retain(|&c| c != id);
            }
        }
        v.nodes[id] = None;
    }
    v.home = None;
    DIRTY.store(false, Ordering::Release);
}

/// Özgargan bloklarni va metadata ni diskka şifrlab yozadi (copy-on-write).
/// Natija: yozilgan bloklar soni.
pub fn commit() -> Result<u64, zaminfs::ZfsError> {
    if !zaminfs::ready() {
        return Err(zaminfs::ZfsError::NoDisk);
    }
    let mut v = vfs_as_kernel();
    DIRTY.store(false, Ordering::Release);
    let ids: Vec<NodeId> = (0..v.nodes.len()).filter(|&id| v.persistent(id)).collect();
    for id in ids {
        if let Some(Node { data: Data::File(d), .. }) = v.nodes[id].as_mut() {
            for c in d.chunks.iter_mut().filter(|c| c.dirty) {
                let Some(data) = c.cache.as_ref() else { continue };
                let new = zaminfs::write_data(data)?;
                if let Some(old) = c.disk.replace(new) {
                    zaminfs::release(old.block);
                }
                c.dirty = false;
            }
        }
    }
    let mut meta = Vec::from(META_MAGIC);
    v.serialize_node(ROOT, &mut meta);
    let r = zaminfs::commit(&meta);
    v.maybe_evict();
    if r.is_err() {
        DIRTY.store(true, Ordering::Release); // keyinroq qayta urinamiz
    }
    r
}

/// Diskdan öqilgan metadata dan papka va fayllarni tiklaydi. Natija: yozuvlar soni.
pub fn import_meta(meta: &[u8]) -> Result<usize, FsError> {
    let mut rest = meta.strip_prefix(META_MAGIC).ok_or(FsError::Corrupt)?;
    fn take<'a>(rest: &mut &'a [u8], n: usize) -> Result<&'a [u8], FsError> {
        if rest.len() < n {
            return Err(FsError::Corrupt);
        }
        let (a, b) = rest.split_at(n);
        *rest = b;
        Ok(a)
    }
    fn u16_(r: &mut &[u8]) -> Result<u16, FsError> {
        Ok(u16::from_le_bytes(take(r, 2)?.try_into().unwrap()))
    }
    fn u32_(r: &mut &[u8]) -> Result<u32, FsError> {
        Ok(u32::from_le_bytes(take(r, 4)?.try_into().unwrap()))
    }
    fn u64_(r: &mut &[u8]) -> Result<u64, FsError> {
        Ok(u64::from_le_bytes(take(r, 8)?.try_into().unwrap()))
    }

    let mut v = vfs_as_kernel();
    let mut count = 0;
    while !rest.is_empty() {
        let kind = take(&mut rest, 1)?[0];
        let plen = u16_(&mut rest)? as usize;
        let path = core::str::from_utf8(take(&mut rest, plen)?).map_err(|_| FsError::Corrupt)?;
        let (owner, mode, ctime, mtime) = (u32_(&mut rest)?, u16_(&mut rest)?, u64_(&mut rest)?, u64_(&mut rest)?);
        let data = match kind {
            REC_DIR => None,
            REC_FILE => {
                let size = u64_(&mut rest)? as usize;
                let n = u32_(&mut rest)? as usize;
                if n != size.div_ceil(file::CHUNK) {
                    return Err(FsError::Corrupt);
                }
                let mut chunks = Vec::with_capacity(n);
                for _ in 0..n {
                    let block = u32_(&mut rest)?;
                    let nonce: [u8; 12] = take(&mut rest, 12)?.try_into().unwrap();
                    let disk = if block == zaminfs::NO_BLOCK {
                        None
                    } else {
                        if !zaminfs::mark_used(block) {
                            return Err(FsError::Corrupt);
                        }
                        Some(zaminfs::BlockRef { block, nonce })
                    };
                    chunks.push(file::Chunk { disk, cache: None, dirty: false });
                }
                Some(FileData { size, chunks })
            }
            _ => return Err(FsError::Corrupt),
        };
        let id = match v.lookup(path) {
            Ok(id) => id,
            Err(_) => {
                let (parent, name) = v.split(path)?;
                let d = if data.is_some() { Data::File(FileData::new()) } else { Data::Dir(Vec::new()) };
                v.add(parent, name, d, false)?
            }
        };
        let node = v.node_mut(id)?;
        if let Some(fd) = data {
            node.data = Data::File(fd);
        }
        node.owner = owner;
        node.mode = mode;
        node.ctime = ctime;
        node.mtime = mtime;
        count += 1;
    }
    DIRTY.store(false, Ordering::Release);
    Ok(count)
}
