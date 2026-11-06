//! Kiçik HTML tahlilçisi — noldan. Hujjatni bloklar (sarlavha, xatboşi, ro'yxat,
//! kod, iqtibos, chiziq) va ularning ichidagi matn bölaklariga (havola, qalin,
//! kod) aylantiradi. <script>/<style> taşlanadi, belgilar (&amp; &#NN; …) yeçiladi.

use alloc::string::String;
use alloc::vec::Vec;

use crate::css::{self, ElRef, Rule, Sty};

#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    H(u8),
    P,
    Li(u8, u16), // çuqurlik, tartib raqami (0 — nuqta)
    Pre,
    Quote,
    Hr,
    Img,
    Indent(u8), // dd va h.k. — belgisiz çekiniş
}

#[derive(Clone)]
pub struct Span {
    pub text: String,
    pub link: Option<String>,
    pub bold: bool,
    pub code: bool,
    pub br: bool, // qator uziliş
    pub color: Option<u32>,
    pub size: f32, // CSS piksel
    pub el: u16,   // eng yaqin ro'yxatdagi element (id / onclick) — JS uçun
    pub button: bool,
    pub underline: bool,
    pub input: u16, // forma maydoni (Doc::inputs indeksi) yoki NO_EL
    pub fx: u16,    // effektlar: FX_* bitlari
}

pub const FX_ITALIC: u16 = 1;
pub const FX_STRIKE: u16 = 2;
pub const FX_UNDER: u16 = 4;
pub const FX_SUP: u16 = 8;
pub const FX_SUB: u16 = 16;
pub const FX_KBD: u16 = 32;
pub const FX_MARK: u16 = 64;
pub const FX_DOTTED: u16 = 128;

#[derive(Clone)]
pub struct Block {
    pub kind: Kind,
    pub spans: Vec<Span>,
    pub bg: Option<u32>,
    pub align: u8, // yakuniy: 0 — çap, 1 — markaz, 2 — öng, 3 — justify
    pub rtl: bool, // dir="rtl" / direction: rtl
    pub size: f32,
    pub color: Option<u32>,
    pub el: u16,
    pub src: String, // rasm manzili (Img)
    pub hidden: bool,
    pub bgk: u32,    // fonni bergan element (qöşni bloklar bitta fon bilan birlaşadi)
    // Quti modeli: konteynerdan (pad, çegara, radius, kenglik) va blokning özidan (chekinişlar)
    pub pad: f32,
    pub bw: f32,
    pub bc: Option<u32>,
    pub radius: f32,
    pub grad: Option<css::Grad>,     // konteyner foni: linear-gradient
    pub shadow: Option<css::Shadow>, // konteyner soyasi: box-shadow
    pub maxw: Option<(f32, bool)>,
    pub center: bool,
    pub mt: f32,
    pub mb: f32,
    pub bmt: f32, // konteynerning öz chekinişlari
    pub bmb: f32,
    // Flex / grid / jadval: konteyner va ustun (bevosita bola) raqamlari
    pub flex: u32,
    pub fcol: u32,
    pub fmode: u8, // 1 — qator (flex/jadval), 3 — grid
    pub fgap: f32,
    pub gcols: u8,
    pub fjust: u8,
    pub fw: Option<(f32, bool)>, // ustun kengligi (bolaning width)
    pub fgrow: f32,
    pub mark: String,   // ro'yxat belgisi ("a.", "iv.", "3.")
    pub det: u16,       // <details> raqami (NO_EL — yöq)
    pub summary: bool,  // <summary> qatori
    pub iw: u16,        // rasm: width/height atributlari (0 — yöq)
    pub ih: u16,
    pub rspan: u8,      // jadval katagi: rowspan (1 — oddiy)
    // position / float: oqimdan taşqari element (eng yaqin absolute/fixed/float ajdod)
    pub oser: u32,               // uning raqami (0 — yöq)
    pub okind: u8,               // 1 — float çap, 2 — float öng, 3 — absolute, 4 — fixed
    pub ow: Option<(f32, bool)>, // uning kengligi
    pub oins: [Option<f32>; 4],  // top, right, bottom, left
    pub pk: u32,                 // absolute uçun konteyner (eng yaqin joylaşgan ajdod; 0 — sahifa)
    pub aser: u32,               // eng yaqin joylaşgan (static emas) ajdod — langar sifatida
    pub rdx: f32,                // position: relative siljişlari yig'indisi (matn)
    pub rdy: f32,
    pub bdx: f32,                // ... va quti foni uçun
    pub bdy: f32,
    pub clear: u8,
}

impl Block {
    /// innerHTML: yangi bloklar eski blokning position/float kontekstini oladi.
    fn copy_pos(&mut self, t: &Block) {
        if self.oser == 0 {
            (self.oser, self.okind, self.ow, self.oins, self.pk) = (t.oser, t.okind, t.ow, t.oins, t.pk);
        }
        if self.aser == 0 {
            self.aser = t.aser;
        }
        (self.rdx, self.rdy, self.bdx, self.bdy) = (self.rdx + t.rdx, self.rdy + t.rdy, self.bdx + t.bdx, self.bdy + t.bdy);
    }
}

/// Forma maydoni.
#[derive(Clone)]
pub struct Input {
    pub kind: u8, // 0 — matn, 1 — yuboriş tugmasi, 2 — belgilaş (checkbox), 3 — parol, 4 — yaşirin, 5 — <button>
    pub name: String,
    pub value: String,
    pub placeholder: String,
    pub form: u16,
    pub el: u16,
    pub size: u16,
    pub checked: bool,
    pub options: Vec<(String, String)>, // <select>: (qiymat, yozuv)
    pub radio: bool,  // radio — bir guruhda (name) faqat bittasi
    pub min: f32,     // range / progress / meter
    pub max: f32,
}

#[derive(Clone)]
pub struct Form {
    pub action: String,
    pub method: String,
    pub el: u16, // JS (onsubmit / addEventListener('submit')) uçun
}

/// JS körinadigan element (id yoki onclick bölganlar).
#[derive(Clone)]
pub struct ElInfo {
    pub id: String,
    pub tag: String,
    pub onclick: Option<String>,
    pub classes: Vec<String>,
    pub parent: u16,
    pub label_for: String, // <label for="…">
    pub style: String,     // inline style="…" (qayta uslublaşda saqlanadi)
}

#[derive(Clone)]
pub enum Script {
    Inline(String),
    Src(String),
}

pub struct Doc {
    pub title: String,
    pub blocks: Vec<Block>,
    pub elems: Vec<ElInfo>,
    pub scripts: Vec<Script>,
    pub bg: Option<u32>,
    pub color: Option<u32>,
    pub inputs: Vec<Input>,
    pub forms: Vec<Form>,
    pub rules: Vec<Rule>, // classList özgarişida qayta uslublaş uçun
    pub svgs: Vec<String>,         // ichki <svg> belgilaşlari (src = "svg:N")
    pub details: Vec<bool>,        // <details> oçiqmi
    pub base: Option<String>,      // <base href>
    pub refresh: Option<(u32, String)>, // <meta http-equiv=refresh>
    pub hovered: Vec<u16>,         // siçqonça ostidagi element va uning ajdodlari (:hover)
    pub media_on: Vec<bool>,       // @media qoidalarining tahlil paytidagi holati
    pub src: String,               // @media bölsa: qayta tahlil uçun HTML manba
    pub extra_css: String,         // … va tashqi CSS
    pub canvases: Vec<crate::canvas::Canvas>, // <canvas> buferlari (src = "canvas:N")
}

pub const NO_EL: u16 = u16::MAX;

impl Doc {
    pub fn empty() -> Doc {
        Doc { title: String::new(), blocks: Vec::new(), elems: Vec::new(), scripts: Vec::new(), bg: None, color: None, inputs: Vec::new(), forms: Vec::new(), rules: Vec::new(), svgs: Vec::new(), details: Vec::new(), base: None, refresh: None, hovered: Vec::new(), media_on: Vec::new(), src: String::new(), extra_css: String::new(), canvases: Vec::new() }
    }
    /// <details> ni oçadi / yopadi.
    pub fn toggle_details(&mut self, d: u16) {
        let Some(open) = self.details.get_mut(d as usize) else { return };
        *open = !*open;
        let o = *open;
        for b in self.blocks.iter_mut().filter(|b| b.det == d && !b.summary) {
            b.hidden = !o;
        }
    }
    /// Elementga tegişli forma maydoni (input / textarea).
    pub fn input_of(&self, el: usize) -> Option<usize> {
        self.inputs.iter().position(|i| i.el as usize == el)
    }
    pub fn find_id(&self, id: &str) -> Option<usize> {
        self.elems.iter().position(|e| e.id == id)
    }
    /// `a` element `e` içidami (yoki özimi)?
    pub fn within(&self, a: u16, e: u16) -> bool {
        let mut cur = a;
        let mut n = 0;
        while cur != NO_EL && n < 512 {
            if cur == e {
                return true;
            }
            cur = self.elems.get(cur as usize).map(|x| x.parent).unwrap_or(NO_EL);
            n += 1;
        }
        false
    }
    /// Elementning matni (barça avlodlari bilan).
    pub fn text_of(&self, el: usize) -> String {
        let e = el as u16;
        let mut out = String::new();
        for b in &self.blocks {
            for s in b.spans.iter().filter(|s| self.within(s.el, e)) {
                out.push_str(&s.text);
            }
        }
        out
    }
    /// Element tarkibini almaştiradi: matn (textContent) yoki HTML (innerHTML).
    pub fn set_content(&mut self, el: usize, text: &str, html: bool) {
        let e = el as u16;
        // 1) Blok darajasidagi element: içidagi barça bloklar → yangisi
        let range: Vec<usize> = (0..self.blocks.len()).filter(|&k| self.within(self.blocks[k].el, e)).collect();
        if let (Some(&first), Some(&last)) = (range.first(), range.last()) {
            let is_block = self.blocks[first].el == e || range.len() > 1 || self.blocks[first].spans.iter().all(|s| self.within(s.el, e));
            if is_block {
                let tmpl = self.blocks[first].clone();
                let base_el = tmpl.el;
                let mut new_blocks: Vec<Block> = if html {
                    let mut d = parse_frag(text, &self.rules);
                    let off = self.elems.len() as u16;
                    for x in d.elems.iter_mut() {
                        x.parent = if x.parent == NO_EL { e } else { x.parent + off };
                    }
                    let remap = |v: u16| if v == NO_EL { e } else { v + off };
                    for b in d.blocks.iter_mut() {
                        b.el = remap(b.el);
                        for sp in b.spans.iter_mut() {
                            sp.el = remap(sp.el);
                        }
                        if b.bg.is_none() {
                            b.bg = tmpl.bg;
                            b.bgk = tmpl.bgk;
                            b.grad = tmpl.grad;
                        }
                        b.shadow = tmpl.shadow;
                        b.flex = tmpl.flex;
                        b.fcol = tmpl.fcol;
                        b.fmode = tmpl.fmode;
                        b.fgap = tmpl.fgap;
                        b.gcols = tmpl.gcols;
                        b.fw = tmpl.fw;
                        b.copy_pos(&tmpl);
                        b.pad = tmpl.pad;
                        b.bw = tmpl.bw;
                        b.bc = tmpl.bc;
                        b.radius = tmpl.radius;
                        b.maxw = tmpl.maxw;
                        b.center = tmpl.center;
                        if b.color.is_none() {
                            b.color = tmpl.color;
                        }
                    }
                    self.adopt_canvases(&mut d);
                    self.elems.extend(d.elems);
                    d.blocks
                } else {
                    Vec::new()
                };
                if new_blocks.is_empty() {
                    let mut b = tmpl.clone();
                    let sp = tmpl.spans.iter().find(|s| !s.br && s.input == NO_EL).cloned().unwrap_or(Span {
                        text: String::new(), link: None, bold: false, code: false, br: false, color: None, size: tmpl.size, el: base_el, button: false, underline: false, input: NO_EL, fx: 0,
                    });
                    let lines: Vec<&str> = text.split('\n').collect();
                    b.spans.clear();
                    for (k, l) in lines.iter().enumerate() {
                        if k > 0 {
                            b.spans.push(Span { text: String::new(), br: true, ..sp.clone() });
                        }
                        b.spans.push(Span { text: String::from(*l), br: false, el: base_el, input: NO_EL, ..sp.clone() });
                    }
                    new_blocks.push(b);
                }
                // Faqat ketma-ket oraliqni almaştiramiz
                let last = if range.windows(2).all(|w| w[1] == w[0] + 1) { last } else { first };
                self.blocks.splice(first..=last, new_blocks);
                return;
            }
        }
        // 2) Qator içidagi element (span, b, a…)
        let plain = if html { strip_tags(text) } else { String::from(text) };
        for bi in 0..self.blocks.len() {
            let pos: Vec<usize> = (0..self.blocks[bi].spans.len()).filter(|&k| self.within(self.blocks[bi].spans[k].el, e)).collect();
            if let Some(&first) = pos.first() {
                let b = &mut self.blocks[bi];
                b.spans[first].text = plain.clone();
                b.spans[first].el = e;
                let mut k = 0;
                b.spans.retain(|_| {
                    let keep = !pos.contains(&k) || k == first;
                    k += 1;
                    keep
                });
                return;
            }
        }
    }
    /// JS: element.style.X = v — elementning blok/bölaklariga qöllanadi.
    pub fn set_style(&mut self, el: usize, prop: &str, val: &str) {
        let e = el as u16;
        let prop = css::kebab(prop);
        let low = val.to_ascii_lowercase();
        for bi in 0..self.blocks.len() {
            let whole = self.within(self.blocks[bi].el, e);
            let owner = self.blocks[bi].el == e;
            let inside: Vec<bool> = self.blocks[bi].spans.iter().map(|sp| self.within(sp.el, e)).collect();
            let b = &mut self.blocks[bi];
            if whole {
                match prop.as_str() {
                    "display" => b.hidden = low == "none",
                    "background" | "background-color" | "background-image" if owner => {
                        if low.contains("gradient(") {
                            if let Some(g) = css::parse_gradient(&low) {
                                (b.grad, b.bg) = (Some(g), Some(g.stops[0].0));
                            }
                        } else if prop != "background-image" {
                            (b.grad, b.bg) = (None, css::parse_color(val));
                        } else if low.trim() == "none" {
                            b.grad = None;
                        }
                    }
                    "box-shadow" if owner => b.shadow = css::parse_shadow(&low),
                    "text-align" if owner => {
                        let mut st = Sty::root();
                        st.rtl = b.rtl;
                        css::apply(&mut st, "text-align", val, b.size);
                        b.align = css::resolve_align(st.align, b.rtl);
                    }
                    "direction" if owner => b.rtl = low == "rtl",
                    "color" => b.color = css::parse_color(val),
                    "font-size" => {
                        let mut st = Sty::root();
                        css::apply(&mut st, "font-size", val, b.size);
                        b.size = st.size;
                    }
                    _ => {}
                }
            }
            for (k, sp) in b.spans.iter_mut().enumerate() {
                if !(whole || inside[k]) {
                    continue;
                }
                match prop.as_str() {
                    "color" => sp.color = css::parse_color(val),
                    "font-weight" => sp.bold = low == "bold" || low.parse::<u32>().is_ok_and(|n| n >= 600),
                    "font-size" => {
                        let mut st = Sty::root();
                        css::apply(&mut st, "font-size", val, sp.size);
                        sp.size = st.size;
                    }
                    _ => {}
                }
            }
        }
    }
    /// Fragment (innerHTML / append) dagi <canvas> buferlarini asosiy hujjatga
    /// köçiradi va bloklardagi "canvas:N" raqamlarini yangilaydi.
    fn adopt_canvases(&mut self, d: &mut Doc) {
        for b in d.blocks.iter_mut() {
            let Some(n) = b.src.strip_prefix("canvas:").and_then(|n| n.parse::<usize>().ok()) else { continue };
            match d.canvases.get_mut(n) {
                Some(cv) if self.canvases.len() < 16 => {
                    let mut c = core::mem::replace(cv, crate::canvas::Canvas::new(NO_EL, 1, 1));
                    c.el = b.el;
                    b.src = alloc::format!("canvas:{}", self.canvases.len());
                    self.canvases.push(c);
                }
                _ => b.src.clear(),
            }
        }
    }
    /// JS: appendChild / insertAdjacentHTML — HTML ni element oxiriga qöşadi.
    pub fn append_into(&mut self, el: usize, src: &str) {
        let e = el as u16;
        let last = (0..self.blocks.len()).rev().find(|&k| self.within(self.blocks[k].el, e));
        let mut d = parse_frag(src, &self.rules);
        let off = self.elems.len() as u16;
        for x in d.elems.iter_mut() {
            x.parent = if x.parent == NO_EL { e } else { x.parent + off };
        }
        let tmpl = last.map(|k| self.blocks[k].clone());
        for b in d.blocks.iter_mut() {
            b.el = if b.el == NO_EL { e } else { b.el + off };
            for sp in b.spans.iter_mut() {
                sp.el = if sp.el == NO_EL { e } else { sp.el + off };
            }
            if let Some(t) = &tmpl {
                if b.bg.is_none() {
                    b.bg = t.bg;
                    b.bgk = t.bgk;
                    b.grad = t.grad;
                }
                b.shadow = t.shadow;
                b.pad = t.pad;
                b.bw = t.bw;
                b.bc = t.bc;
                b.radius = t.radius;
                b.maxw = t.maxw;
                b.center = t.center;
                b.flex = t.flex;
                b.fcol = t.fcol;
                b.fmode = t.fmode;
                b.fgap = t.fgap;
                b.gcols = t.gcols;
                b.fw = t.fw;
                b.copy_pos(t);
            }
        }
        self.adopt_canvases(&mut d);
        self.elems.extend(d.elems);
        let at = last.map(|k| k + 1).unwrap_or(self.blocks.len());
        self.blocks.splice(at..at, d.blocks);
    }
    /// Element zanjiri: ildizdan elementning özigaça (elems indekslari).
    fn chain(&self, el: usize) -> Vec<usize> {
        let mut ch = Vec::new();
        let mut k = el;
        while k < self.elems.len() && ch.len() < 512 {
            ch.push(k);
            let p = self.elems[k].parent as usize;
            if p == NO_EL as usize || p >= k {
                break;
            }
            k = p;
        }
        ch.reverse();
        ch
    }
    /// Elementning hisoblangan uslubi: ajdodlar zanjiri böyiça (meros, avlod tanlagiçlari, :hover bilan).
    fn style_of(&self, el: usize) -> Sty {
        let ch = self.chain(el);
        let mut parent = Sty::root();
        let mut anc: Vec<ElRef> = Vec::new();
        for &k in &ch {
            let info = &self.elems[k];
            let me = ElRef { tag: &info.tag, id: &info.id, classes: &info.classes, hover: self.hovered.contains(&(k as u16)) };
            let inline = if info.style.is_empty() { None } else { Some(info.style.as_str()) };
            let st = css::compute(&parent, &self.rules, &me, &anc, inline);
            if k == el {
                return st;
            }
            parent = st;
            anc.push(me);
        }
        parent
    }
    /// Siçqonça `el` ustida (NO_EL — hech narsa): :hover qoidalari tegişli elementlarga qayta qöllanadi.
    /// Qaytaradi: nimadir özgardimi.
    pub fn set_hover(&mut self, el: u16) -> bool {
        let new: Vec<u16> = if el == NO_EL { Vec::new() } else { self.chain(el as usize).into_iter().map(|k| k as u16).collect() };
        if new == self.hovered {
            return false;
        }
        let (mut last, mut inner) = (false, false);
        for r in &self.rules {
            let (l, i) = css::hover_kind(r);
            last |= l;
            inner |= i;
        }
        let diff: Vec<u16> = self.hovered.iter().filter(|k| !new.contains(k)).chain(new.iter().filter(|k| !self.hovered.contains(k))).copied().collect();
        self.hovered = new;
        if !last && !inner {
            return false;
        }
        let mut todo: Vec<usize> = Vec::new();
        if last {
            todo.extend(diff.iter().map(|&k| k as usize));
        }
        if inner {
            // ".karta:hover p" — özgargan elementlarning avlodlari ham
            for k in 0..self.elems.len().min(20_000) {
                if diff.iter().any(|&d| d as usize != k && self.within(k as u16, d)) && !todo.contains(&k) {
                    todo.push(k);
                }
            }
        }
        let mut any = false;
        for k in todo {
            let hit = self.elems.get(k).is_some_and(|e| css::hover_hits(&self.rules, &e.tag, &e.id, &e.classes));
            if hit {
                self.restyle(k);
                any = true;
            }
        }
        any
    }
    /// classList / :hover özgardi: elementning qoidalari (rang, fon, ko'rinişi) qayta qöllanadi.
    pub fn restyle(&mut self, el: usize) {
        if el >= self.elems.len() {
            return;
        }
        let st = self.style_of(el);
        let e = el as u16;
        let base = Sty::root();
        for bi in 0..self.blocks.len() {
            let whole = self.blocks[bi].el == e;
            if whole {
                self.blocks[bi].hidden = st.hidden;
                if st.bg.is_some() || self.blocks[bi].bgk == 0 {
                    self.blocks[bi].bg = st.bg;
                }
            }
            for k in 0..self.blocks[bi].spans.len() {
                if whole || self.blocks[bi].spans[k].el == e {
                    let sp = &mut self.blocks[bi].spans[k];
                    sp.color = st.color;
                    sp.bold = st.bold;
                    if let Some(u) = st.underline {
                        sp.underline = u;
                    }
                    if st.size != base.size {
                        sp.size = st.size;
                    }
                }
            }
        }
        // Yaşiriş butun avlod bloklariga
        for bi in 0..self.blocks.len() {
            if self.blocks[bi].el != e && self.within(self.blocks[bi].el, e) {
                self.blocks[bi].hidden = st.hidden;
            }
        }
    }
    /// Oddiy tanlagiç böyiça elementlar: "#id", ".klass", "teg", "teg.klass".
    pub fn query_all(&self, sel: &str) -> Vec<usize> {
        let sel = sel.trim();
        let last = sel.split_whitespace().last().unwrap_or(sel);
        let (tag, rest) = match last.find(['.', '#']) {
            Some(0) => ("", last),
            Some(i) => (&last[..i], &last[i..]),
            None => (last, ""),
        };
        let mut id = String::new();
        let mut classes: Vec<String> = Vec::new();
        let mut cur = String::new();
        let mut kind = ' ';
        for c in rest.chars().chain(core::iter::once('.')) {
            if c == '.' || c == '#' {
                if !cur.is_empty() {
                    if kind == '#' {
                        id = core::mem::take(&mut cur);
                    } else {
                        classes.push(core::mem::take(&mut cur));
                    }
                }
                cur.clear();
                kind = c;
            } else {
                cur.push(c);
            }
        }
        (0..self.elems.len())
            .filter(|&k| {
                let e = &self.elems[k];
                (tag.is_empty() || tag == "*" || e.tag == tag.to_ascii_lowercase())
                    && (id.is_empty() || e.id == id)
                    && classes.iter().all(|c| e.classes.iter().any(|x| x == c))
            })
            .collect()
    }
    /// JS: document.write — HTML bölagini oxiriga qöşadi.
    pub fn append_html(&mut self, src: &str) {
        let d = parse(src);
        self.blocks.extend(d.blocks);
    }
}

/// Nomli HTML belgilari (alifbo tartibida — ikkilik qidiruv).
static ENTITIES: &[(&str, &str)] = &[
    ("AElig", "Æ"),
    ("Aacute", "Á"),
    ("Acirc", "Â"),
    ("Agrave", "À"),
    ("Alpha", "Α"),
    ("Aring", "Å"),
    ("Atilde", "Ã"),
    ("Auml", "Ä"),
    ("Beta", "Β"),
    ("Ccedil", "Ç"),
    ("Chi", "Χ"),
    ("Dagger", "‡"),
    ("Delta", "Δ"),
    ("ETH", "Ð"),
    ("Eacute", "É"),
    ("Ecirc", "Ê"),
    ("Egrave", "È"),
    ("Epsilon", "Ε"),
    ("Eta", "Η"),
    ("Euml", "Ë"),
    ("Gamma", "Γ"),
    ("Iacute", "Í"),
    ("Icirc", "Î"),
    ("Igrave", "Ì"),
    ("Iota", "Ι"),
    ("Iuml", "Ï"),
    ("Kappa", "Κ"),
    ("Lambda", "Λ"),
    ("Mu", "Μ"),
    ("Ntilde", "Ñ"),
    ("Nu", "Ν"),
    ("OElig", "Œ"),
    ("Oacute", "Ó"),
    ("Ocirc", "Ô"),
    ("Ograve", "Ò"),
    ("Omega", "Ω"),
    ("Omicron", "Ο"),
    ("Oslash", "Ø"),
    ("Otilde", "Õ"),
    ("Ouml", "Ö"),
    ("Phi", "Φ"),
    ("Pi", "Π"),
    ("Prime", "″"),
    ("Psi", "Ψ"),
    ("Rho", "Ρ"),
    ("Scaron", "Š"),
    ("Sigma", "Σ"),
    ("THORN", "Þ"),
    ("Tau", "Τ"),
    ("Theta", "Θ"),
    ("Uacute", "Ú"),
    ("Ucirc", "Û"),
    ("Ugrave", "Ù"),
    ("Upsilon", "Υ"),
    ("Uuml", "Ü"),
    ("Xi", "Ξ"),
    ("Yacute", "Ý"),
    ("Yuml", "Ÿ"),
    ("Zeta", "Ζ"),
    ("aacute", "á"),
    ("acirc", "â"),
    ("acute", "´"),
    ("aelig", "æ"),
    ("agrave", "à"),
    ("alpha", "α"),
    ("amp", "&"),
    ("and", "∧"),
    ("ang", "∠"),
    ("apos", "'"),
    ("aring", "å"),
    ("asymp", "≈"),
    ("atilde", "ã"),
    ("auml", "ä"),
    ("bdquo", "„"),
    ("beta", "β"),
    ("brvbar", "¦"),
    ("bull", "•"),
    ("cap", "∩"),
    ("ccedil", "ç"),
    ("cedil", "¸"),
    ("cent", "¢"),
    ("check", "✓"),
    ("chi", "χ"),
    ("circ", "ˆ"),
    ("clubs", "♣"),
    ("cong", "≅"),
    ("copy", "©"),
    ("crarr", "↵"),
    ("cross", "✗"),
    ("cup", "∪"),
    ("curren", "¤"),
    ("dArr", "⇓"),
    ("dagger", "†"),
    ("darr", "↓"),
    ("deg", "°"),
    ("delta", "δ"),
    ("diams", "♦"),
    ("divide", "÷"),
    ("eacute", "é"),
    ("ecirc", "ê"),
    ("egrave", "è"),
    ("empty", "∅"),
    ("emsp", " "),
    ("ensp", " "),
    ("epsilon", "ε"),
    ("equiv", "≡"),
    ("eta", "η"),
    ("eth", "ð"),
    ("euml", "ë"),
    ("euro", "€"),
    ("exist", "∃"),
    ("fnof", "ƒ"),
    ("forall", "∀"),
    ("frac12", "½"),
    ("frac14", "¼"),
    ("frac34", "¾"),
    ("frasl", "⁄"),
    ("gamma", "γ"),
    ("ge", "≥"),
    ("gt", ">"),
    ("hArr", "⇔"),
    ("harr", "↔"),
    ("hearts", "♥"),
    ("hellip", "…"),
    ("iacute", "í"),
    ("icirc", "î"),
    ("iexcl", "¡"),
    ("igrave", "ì"),
    ("infin", "∞"),
    ("int", "∫"),
    ("iota", "ι"),
    ("iquest", "¿"),
    ("isin", "∈"),
    ("iuml", "ï"),
    ("kappa", "κ"),
    ("lArr", "⇐"),
    ("lambda", "λ"),
    ("laquo", "«"),
    ("larr", "←"),
    ("ldquo", "“"),
    ("le", "≤"),
    ("lowast", "∗"),
    ("loz", "◊"),
    ("lrm", "\u{200b}"),
    ("lsaquo", "‹"),
    ("lsquo", "‘"),
    ("lt", "<"),
    ("macr", "¯"),
    ("mdash", "—"),
    ("micro", "µ"),
    ("middot", "·"),
    ("minus", "−"),
    ("mu", "μ"),
    ("nabla", "∇"),
    ("nbsp", " "),
    ("ndash", "–"),
    ("ne", "≠"),
    ("ni", "∋"),
    ("not", "¬"),
    ("notin", "∉"),
    ("ntilde", "ñ"),
    ("nu", "ν"),
    ("oacute", "ó"),
    ("ocirc", "ô"),
    ("oelig", "œ"),
    ("ograve", "ò"),
    ("oline", "‾"),
    ("omega", "ω"),
    ("omicron", "ο"),
    ("oplus", "⊕"),
    ("or", "∨"),
    ("ordf", "ª"),
    ("ordm", "º"),
    ("oslash", "ø"),
    ("otilde", "õ"),
    ("otimes", "⊗"),
    ("ouml", "ö"),
    ("para", "¶"),
    ("part", "∂"),
    ("permil", "‰"),
    ("perp", "⊥"),
    ("phi", "φ"),
    ("pi", "π"),
    ("plusmn", "±"),
    ("pound", "£"),
    ("prime", "′"),
    ("prod", "∏"),
    ("prop", "∝"),
    ("psi", "ψ"),
    ("quot", "\""),
    ("rArr", "⇒"),
    ("radic", "√"),
    ("raquo", "»"),
    ("rarr", "→"),
    ("rdquo", "”"),
    ("reg", "®"),
    ("rho", "ρ"),
    ("rlm", "\u{200b}"),
    ("rsaquo", "›"),
    ("rsquo", "’"),
    ("sbquo", "‚"),
    ("scaron", "š"),
    ("sdot", "⋅"),
    ("sect", "§"),
    ("shy", "\u{200b}"),
    ("sigma", "σ"),
    ("sigmaf", "ς"),
    ("sim", "∼"),
    ("spades", "♠"),
    ("star", "☆"),
    ("starf", "★"),
    ("sub", "⊂"),
    ("sube", "⊆"),
    ("sum", "∑"),
    ("sup", "⊃"),
    ("sup1", "¹"),
    ("sup2", "²"),
    ("sup3", "³"),
    ("supe", "⊇"),
    ("szlig", "ß"),
    ("tau", "τ"),
    ("there4", "∴"),
    ("theta", "θ"),
    ("thinsp", " "),
    ("thorn", "þ"),
    ("tilde", "˜"),
    ("times", "×"),
    ("trade", "™"),
    ("uArr", "⇑"),
    ("uacute", "ú"),
    ("uarr", "↑"),
    ("ucirc", "û"),
    ("ugrave", "ù"),
    ("uml", "¨"),
    ("upsilon", "υ"),
    ("uuml", "ü"),
    ("xi", "ξ"),
    ("yacute", "ý"),
    ("yen", "¥"),
    ("yuml", "ÿ"),
    ("zeta", "ζ"),
    ("zwj", "\u{200b}"),
    ("zwnj", "\u{200b}"),
];

fn entity(name: &str) -> Option<char> {
    if let Some(n) = name.strip_prefix('#') {
        let v = if let Some(h) = n.strip_prefix('x').or_else(|| n.strip_prefix('X')) {
            u32::from_str_radix(h, 16).ok()?
        } else {
            n.parse().ok()?
        };
        return char::from_u32(v);
    }
    ENTITIES.binary_search_by(|(n, _)| (*n).cmp(name)).ok().and_then(|k| ENTITIES[k].1.chars().next()).or(match name {
        "zwnj" | "zwj" | "shy" | "lrm" | "rlm" => Some('\u{200b}'),
        _ => None,
    })
}

pub fn decode_entities(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        let tail = &rest[i + 1..];
        match tail.find(';').filter(|&j| j <= 10).and_then(|j| entity(&tail[..j]).map(|c| (c, j))) {
            Some((c, j)) => {
                out.push(c);
                rest = &tail[j + 1..];
            }
            None => {
                out.push('&');
                rest = tail;
            }
        }
    }
    out.push_str(rest);
    out
}

fn attr(tag: &str, name: &str) -> Option<String> {
    let low = tag.to_ascii_lowercase();
    let mut from = 0;
    while let Some(p) = low[from..].find(name) {
        let i = from + p;
        let before_ok = i == 0 || low.as_bytes()[i - 1].is_ascii_whitespace();
        let after = low[i + name.len()..].trim_start();
        if before_ok && after.starts_with('=') {
            let off = tag.len() - after.len() + 1;
            let v = tag[off..].trim_start();
            let val = if let Some(r) = v.strip_prefix('"') {
                r.split('"').next().unwrap_or("")
            } else if let Some(r) = v.strip_prefix('\'') {
                r.split('\'').next().unwrap_or("")
            } else {
                v.split(|c: char| c.is_whitespace() || c == '>').next().unwrap_or("")
            };
            return Some(decode_entities(val));
        }
        from = i + name.len();
    }
    None
}

/// Brauzerning öz uslublari (UA stylesheet).
const UA_CSS: &str = "h1{font-size:2em;font-weight:bold}h2{font-size:1.5em;font-weight:bold}h3{font-size:1.17em;font-weight:bold}\
h4,h5,h6{font-weight:bold}b,strong,th{font-weight:bold}small{font-size:.83em}big{font-size:1.2em}center{text-align:center}\
a{text-decoration:underline}pre,code,kbd,samp,tt{font-size:.9em}button{font-weight:bold}\
tr{display:flex}td,th{padding:3px}th{font-weight:bold}\
h4{font-size:1em}h5{font-size:.83em}h6{font-size:.67em}dt{font-weight:bold}caption{text-align:center;font-weight:bold}\
figcaption{font-size:.85em;text-align:center}fieldset{border:1px solid #808080;padding:8px;border-radius:6px;margin:8px 0}\
legend{font-weight:bold}address{font-style:italic}summary{font-weight:bold}sub,sup{font-size:.75em}kbd{font-size:.9em}\
figure{margin:8px 0}";

const VOID: [&str; 12] = ["br", "hr", "img", "meta", "link", "input", "area", "base", "col", "embed", "source", "wbr"];

struct Open {
    tag: String,
    id: String,
    classes: Vec<String>,
    sty: Sty,
    el: u16,
    link: Option<String>,
    serial: u32,
    input: u16,
    rspan: u8,
}

/// rowspan bilan pastki qatorlarga çozilgan katak.
struct Pend {
    start: usize,
    n: usize,
    left: u8,
    row: u32,
    fw: Option<(f32, bool)>,
    grow: f32,
}

/// Bitta <table> ning qator holati.
#[derive(Default)]
struct TGrid {
    row: u32,
    col: usize,
    in_row: bool,
    pend: Vec<Pend>,
}

struct P {
    doc: Doc,
    rules: Vec<Rule>,
    stack: Vec<Open>,
    cur: Option<Block>,
    pre: u32,
    lists: Vec<(bool, i32, char, bool)>, // (tartiblimi, hisob, turi: 1 a A i I, teskari)
    in_title: bool,
    serial: u32,
    form: u16,
    dets: Vec<(u16, bool)>,          // oçiq <details> steki
    tborder: Vec<bool>,              // <table border> steki
    tgrid: Vec<TGrid>,               // jadval qatorlari: rowspan egallagan ustunlar
    rs_new: bool,                    // oxirgi katak rowspan qöşdi (kengligini yozib qöyiş uçun)
    media: Option<(String, String, String, u16, u16)>, // <video>/<audio>: (turi, manba, poster, width, height)
    pic_src: String,                 // <picture><source srcset>
    depth: u8,                       // iframe ichma-ichligi
}

impl P {
    fn top(&self) -> Sty {
        self.stack.last().map(|o| o.sty).unwrap_or(Sty::root())
    }
    fn el(&self) -> u16 {
        self.stack.last().map(|o| o.el).unwrap_or(NO_EL)
    }
    fn hidden(&self) -> bool {
        self.top().hidden
    }
    fn flush(&mut self) {
        if let Some(b) = self.cur.take() {
            let has = b.spans.iter().any(|s| !s.text.trim().is_empty() || s.input != NO_EL);
            let target = b.el != NO_EL && self.doc.elems.get(b.el as usize).is_some_and(|e| !e.id.is_empty());
            if has || target || b.kind == Kind::Hr || b.kind == Kind::Img {
                self.doc.blocks.push(b);
            }
        }
    }
    fn new_block(&self, k: Kind) -> Block {
        let mut st = self.top();
        // <details>: yopiq bölsa — summary dan taşqari hammasi yaşirin
        let (det, summary) = match self.dets.last() {
            Some(&(d, open)) => {
                let sm = self.stack.iter().any(|o| o.tag == "summary");
                if !open && !sm {
                    st.hidden = true;
                }
                (d, sm)
            }
            None => (NO_EL, false),
        };
        // Fon: eng yaqin fonli ajdod (body/html dan taşqari) — butun konteyner bölib bo'yaladi
        // Eng yaqin flex/grid konteyner va uning bevosita bolasi (ustun)
        let fl = match self.stack.iter().rposition(|o| o.sty.flex == 1 || o.sty.flex == 3) {
            Some(k) => {
                let c = &self.stack[k];
                let (col, fw, grow, rs) = match self.stack.get(k + 1) {
                    Some(ch) => (ch.serial, ch.sty.maxw, ch.sty.grow, ch.rspan),
                    None => (c.serial, None, 0.0, 1),
                };
                (c.serial, col, c.sty.flex, c.sty.gap, c.sty.gcols, c.sty.justify, fw, grow, rs)
            }
            None => (0, 0, 0, 0.0, 0, 0, None, 0.0, 1),
        };
        // Quti: flex konteynerdan içkaridagisi (aks holda butun qator bitta qutiga tuşib qoladi)
        let fk = self.stack.iter().rposition(|o| o.sty.flex == 1 || o.sty.flex == 3).map(|k| k + 1).unwrap_or(0);
        let cont = self.stack[fk.min(self.stack.len())..].iter().rev().take_while(|o| o.tag != "body" && o.tag != "html").find(|o| o.sty.is_box());
        let (bg, bgk, cs) = match cont {
            Some(o) => (o.sty.bg, o.serial, o.sty),
            None => (None, 0, Sty::root()),
        };
        // position / float
        let oi = self.stack.iter().rposition(|o| o.sty.float != 0 || o.sty.pos >= 2);
        let (oser, okind, ow, oins, pk) = match oi {
            Some(i) => {
                let o = &self.stack[i];
                let kind = match o.sty.pos {
                    2 => 3,
                    3 => 4,
                    _ => o.sty.float,
                };
                let pk = self.stack[..i].iter().rev().find(|a| a.sty.pos != 0).map(|a| a.serial).unwrap_or(0);
                (o.serial, kind, o.sty.maxw, o.sty.ins, pk)
            }
            None => (0, 0, None, [None; 4], 0),
        };
        let aser = self.stack.iter().rev().find(|a| a.sty.pos != 0).map(|a| a.serial).unwrap_or(0);
        let off = |o: &Open| -> (f32, f32) {
            if o.sty.pos != 1 {
                return (0.0, 0.0);
            }
            let i = o.sty.ins;
            (i[3].unwrap_or(-i[1].unwrap_or(0.0)), i[0].unwrap_or(-i[2].unwrap_or(0.0)))
        };
        let (mut rdx, mut rdy, mut bdx, mut bdy) = (0.0, 0.0, 0.0, 0.0);
        for o in &self.stack {
            let (dx, dy) = off(o);
            rdx += dx;
            rdy += dy;
            if o.serial <= bgk {
                bdx += dx;
                bdy += dy;
            }
        }
        Block {
            kind: k, spans: Vec::new(), bg, align: css::resolve_align(st.align, st.rtl), rtl: st.rtl, size: st.size, color: st.color, el: self.el(), src: String::new(), hidden: st.hidden, bgk,
            pad: cs.pad, bw: cs.bw, bc: cs.bc, radius: cs.radius, grad: cs.grad, shadow: cs.shadow, maxw: cs.maxw, center: cs.center, mt: st.mt, mb: st.mb,
            bmt: cs.mt, bmb: cs.mb,
            flex: fl.0, fcol: fl.1, fmode: fl.2, fgap: fl.3, gcols: fl.4, fjust: fl.5, fw: fl.6, fgrow: fl.7,
            mark: String::new(), det, summary, iw: 0, ih: 0, rspan: fl.8,
            oser, okind, ow, oins, pk, aser, rdx, rdy, bdx, bdy, clear: st.clear,
        }
    }
    /// Böş (void) element (img) uçun float / position: uslubi hisoblanib, blok oqimdan çiqariladi.
    fn void_pos(&mut self, blk: &mut Block, name: &str, tag: &str) {
        let id = attr(tag, "id").unwrap_or_default();
        let classes: Vec<String> = attr(tag, "class").map(|c| c.split_whitespace().map(String::from).collect()).unwrap_or_default();
        let st = {
            let anc: Vec<ElRef> = self.stack.iter().map(|o| ElRef { tag: &o.tag, id: &o.id, classes: &o.classes, hover: false }).collect();
            let me = ElRef { tag: name, id: &id, classes: &classes, hover: false };
            css::compute(&self.top(), &self.rules, &me, &anc, attr(tag, "style").as_deref())
        };
        let kind = match st.pos {
            2 => 3,
            3 => 4,
            _ => st.float,
        };
        if kind != 0 {
            self.serial += 1;
            blk.oser = self.serial;
            blk.okind = kind;
            blk.ow = st.maxw;
            blk.oins = st.ins;
            blk.pk = blk.aser;
        } else if st.pos == 1 {
            blk.rdx += st.ins[3].unwrap_or(-st.ins[1].unwrap_or(0.0));
            blk.rdy += st.ins[0].unwrap_or(-st.ins[2].unwrap_or(0.0));
        }
        blk.clear = st.clear;
    }
    /// `<iframe srcdoc>` (yoki yuklangan bir xil manbali `src`): ichki hujjat alohida — öz CSS i
    /// bilan — tahlil qilinadi va ramka-quti içida ota hujjat bloklari orasiga joylanadi.
    /// Ichki elementlar ota JS dan ajratilgan (id/klass/onclick yöq), ichki skriptlar bajarilmaydi.
    fn embed_frame(&mut self, html: &str, tag: &str, fel: u16) {
        self.flush();
        if self.depth >= 3 {
            return;
        }
        let inner = parse_doc(html, "", self.depth + 1);
        let tmpl = self.new_block(Kind::P);
        let parse_w = |w: &str| -> Option<(f32, bool)> {
            let w = w.trim();
            match w.strip_suffix('%') {
                Some(p) => p.trim().parse::<f32>().ok().map(|v| (v.clamp(1.0, 100.0), true)),
                None => w.trim_end_matches("px").trim().parse::<f32>().ok().filter(|&v| v > 0.0).map(|v| (v, false)),
            }
        };
        let own = self.stack.last().filter(|o| o.tag == "iframe").map(|o| o.sty);
        let width = attr(tag, "width").and_then(|w| parse_w(&w)).or(own.and_then(|s| s.maxw)).or(Some((100.0, true)));
        let border = if attr(tag, "frameborder").is_some_and(|v| v.trim() == "0") { 0.0 } else { own.map(|s| s.bw).filter(|&v| v > 0.0).unwrap_or(2.0) };
        let bc = own.and_then(|s| s.bc).unwrap_or(0x8a94a6);
        let fbg = inner.bg.unwrap_or(0xffffff);
        let fcol = inner.color.unwrap_or(if ((fbg >> 16 & 255) * 3 + (fbg >> 8 & 255) * 6 + (fbg & 255)) / 10 > 128 { 0x1d1d1f } else { 0xe8ecf2 });
        // Raqamlar (element, maydon, svg, canvas, details, konteyner seriyalari) — siljitiladi
        let eoff = self.doc.elems.len();
        let keep = eoff + inner.elems.len() < 60_000;
        let remap = |v: u16| if v == NO_EL || !keep { fel } else { v + eoff as u16 };
        for e in &inner.elems {
            const KEEP: [&str; 5] = ["input", "textarea", "select", "button", "label"];
            let tag = if KEEP.contains(&e.tag.as_str()) { e.tag.clone() } else { String::from("#frame") };
            if keep {
                self.doc.elems.push(ElInfo { id: String::new(), tag, onclick: None, classes: Vec::new(), parent: remap(e.parent), label_for: String::new(), style: String::new() });
            }
        }
        let ioff = self.doc.inputs.len() as u16;
        for mut inp in inner.inputs {
            inp.form = NO_EL;
            inp.el = remap(inp.el);
            self.doc.inputs.push(inp);
        }
        let soff = self.doc.svgs.len();
        self.doc.svgs.extend(inner.svgs);
        let coff = self.doc.canvases.len();
        for mut c in inner.canvases {
            c.el = remap(c.el);
            self.doc.canvases.push(c);
        }
        let doff = self.doc.details.len() as u16;
        self.doc.details.extend(inner.details);
        let base = self.serial;
        let mut top = 0u32;
        let sh = |v: u32| if v == 0 { 0 } else { v + base };
        let mut blocks = inner.blocks;
        if blocks.is_empty() {
            blocks.push(Block { kind: Kind::P, spans: Vec::new(), ..tmpl.clone() });
        }
        for b in blocks.iter_mut() {
            top = top.max(b.flex).max(b.fcol).max(b.oser).max(b.pk).max(b.aser).max(b.bgk);
            b.el = remap(b.el);
            for sp in b.spans.iter_mut() {
                sp.el = remap(sp.el);
                if sp.input != NO_EL {
                    sp.input += ioff;
                }
            }
            if let Some(n) = b.src.strip_prefix("svg:").and_then(|n| n.parse::<usize>().ok()) {
                b.src = alloc::format!("svg:{}", n + soff);
            } else if let Some(n) = b.src.strip_prefix("canvas:").and_then(|n| n.parse::<usize>().ok()) {
                b.src = alloc::format!("canvas:{}", n + coff);
            }
            b.det = if b.det == NO_EL { tmpl.det } else { b.det + doff };
            b.hidden |= tmpl.hidden;
            if tmpl.flex != 0 {
                // iframe ning özi flex/grid ustunida — ichki flex tekislanadi
                (b.flex, b.fcol, b.fmode, b.fgap, b.gcols, b.fjust, b.fw, b.fgrow, b.rspan) = (tmpl.flex, tmpl.fcol, tmpl.fmode, tmpl.fgap, tmpl.gcols, tmpl.fjust, tmpl.fw, tmpl.fgrow, tmpl.rspan);
            } else {
                (b.flex, b.fcol) = (sh(b.flex), sh(b.fcol));
            }
            (b.oser, b.pk, b.aser) = (sh(b.oser), sh(b.pk), sh(b.aser));
            b.copy_pos(&tmpl);
            if b.color.is_none() {
                b.color = Some(fcol);
            }
        }
        self.serial = base + top + 1;
        let key = self.serial;
        for b in blocks.iter_mut() {
            // Bitta ramka-quti: ichki hujjat foni, çegara, iframe kengligi
            (b.bg, b.bgk, b.pad, b.bw, b.bc, b.radius, b.maxw, b.center, b.bmt, b.bmb) = (Some(fbg), key, 8.0, border, Some(bc), 2.0, width, false, 6.0, 6.0);
            (b.grad, b.shadow) = (None, None);
        }
        self.doc.blocks.extend(blocks);
    }
    fn start(&mut self, k: Kind) {
        self.flush();
        // display:none bölsa ham blok yaratiladi (hidden) — JS keyin körsatişi mumkin
        self.cur = Some(self.new_block(k));
    }
    fn text(&mut self, t: &str) {
        if self.in_title {
            self.doc.title.push_str(&decode_entities(t));
            return;
        }
        // Körinadigan blok içidagi yaşirin qator elementi — taşlanadi
        if self.hidden() && self.cur.as_ref().is_some_and(|b| !b.hidden) {
            return;
        }
        let mut s = decode_entities(t);
        if self.pre == 0 {
            let mut c = String::new();
            let mut sp = false;
            for ch in s.chars() {
                if ch.is_whitespace() {
                    if !sp {
                        c.push(' ');
                    }
                    sp = true;
                } else {
                    c.push(ch);
                    sp = false;
                }
            }
            s = c;
            if s.trim().is_empty() && self.cur.as_ref().is_none_or(|b| b.spans.is_empty()) {
                return;
            }
        }
        if self.cur.is_none() {
            let k = if self.pre > 0 { Kind::Pre } else { Kind::P };
            self.cur = Some(self.new_block(k));
        }
        let st = self.top();
        let link = self.stack.iter().rev().find_map(|o| o.link.clone());
        let code = self.pre > 0 || self.stack.iter().any(|o| matches!(o.tag.as_str(), "code" | "kbd" | "samp" | "tt"));
        let button = self.stack.iter().any(|o| o.tag == "button");
        let input = self.stack.iter().rev().find(|o| o.input != NO_EL).map(|o| o.input).unwrap_or(NO_EL);
        let mut fx = 0u16;
        for o in &self.stack {
            fx |= match o.tag.as_str() {
                "i" | "em" | "cite" | "var" | "dfn" | "address" => FX_ITALIC,
                "u" | "ins" => FX_UNDER,
                "s" | "strike" | "del" => FX_STRIKE,
                "sup" => FX_SUP,
                "sub" => FX_SUB,
                "kbd" => FX_KBD,
                "mark" => FX_MARK,
                "abbr" | "acronym" => FX_DOTTED,
                _ => 0,
            };
        }
        if st.italic {
            fx |= FX_ITALIC;
        }
        if st.strike {
            fx |= FX_STRIKE;
        }
        if st.underline == Some(true) {
            fx |= FX_UNDER;
        }
        let el = self.el();
        let b = self.cur.as_mut().unwrap();
        if b.spans.is_empty() && self.pre == 0 {
            s = String::from(s.trim_start());
        }
        let underline = st.underline.unwrap_or(false) && link.is_some();
        b.spans.push(Span { text: s, link, bold: st.bold, code, br: false, color: st.color, size: st.size, el, button, underline, input, fx });
    }
    /// Forma maydonini ro'yxatga oladi. `el` — tayyor element (yöq bölsa id/onclick dan yaratiladi).
    fn add_input(&mut self, kind: u8, tag: &str, value: String, el: Option<u16>) -> u16 {
        let el = el.unwrap_or_else(|| {
            let id = attr(tag, "id").unwrap_or_default();
            let onclick = attr(tag, "onclick");
            let classes: Vec<String> = attr(tag, "class").map(|c| c.split_whitespace().map(String::from).collect()).unwrap_or_default();
            let parent = self.el();
            self.doc.elems.push(ElInfo { id, tag: String::from("input"), onclick, classes, parent, label_for: String::new(), style: attr(tag, "style").unwrap_or_default() });
            (self.doc.elems.len() - 1) as u16
        });
        let size = attr(tag, "size").and_then(|v| v.parse().ok()).unwrap_or(if kind == 0 || kind == 3 { 20 } else { 0 });
        self.doc.inputs.push(Input {
            kind,
            name: attr(tag, "name").unwrap_or_default(),
            value,
            placeholder: attr(tag, "placeholder").unwrap_or_default(),
            form: self.form,
            el,
            size,
            checked: attr(tag, "checked").is_some() || tag.split_whitespace().any(|w| w.eq_ignore_ascii_case("checked")),
            options: Vec::new(),
            radio: false,
            min: attr(tag, "min").and_then(|v| v.trim().parse().ok()).unwrap_or(0.0),
            max: attr(tag, "max").and_then(|v| v.trim().parse().ok()).unwrap_or(if kind == 9 { 1.0 } else { 100.0 }),
        });
        (self.doc.inputs.len() - 1) as u16
    }
    /// Maydonni joriy blokka qöyadi.
    fn input_span(&mut self, idx: u16) {
        if self.doc.inputs[idx as usize].kind == 4 || (self.hidden() && self.cur.as_ref().is_some_and(|b| !b.hidden)) {
            return;
        }
        if self.cur.is_none() {
            self.cur = Some(self.new_block(Kind::P));
        }
        let st = self.top();
        let el = self.doc.inputs[idx as usize].el;
        let b = self.cur.as_mut().unwrap();
        b.spans.push(Span { text: String::from("\u{2007}"), link: None, bold: false, code: false, br: false, color: st.color, size: st.size, el, button: false, underline: false, input: idx, fx: 0 });
    }
    /// Havola matni (media / iframe kartoçkalari uçun).
    /// <audio>/<video> kartoçkasi: video — poster rasmi (bölsa), audio — ijro tugmasi
    /// (`zamin-play:` havolasi; GUI WAV ni virtio-sound orqali çaladi) va yuklab oliş havolasi.
    fn media_card(&mut self, kind: &str, msrc: String, poster: String, mw: u16, mh: u16) {
        self.flush();
        let fname = String::from(msrc.rsplit('/').next().unwrap_or("").split('?').next().unwrap_or(""));
        let fname = if fname.is_empty() || msrc.starts_with("data:") { String::from("media") } else { fname };
        let link = if msrc.is_empty() { None } else { Some(msrc.clone()) };
        if kind == "video" {
            if !poster.is_empty() {
                let mut blk = self.new_block(Kind::Img);
                blk.src = poster;
                blk.iw = mw;
                blk.ih = mh;
                blk.spans.push(Span { text: alloc::format!("▶ {}", fname), link: link.clone(), bold: false, code: false, br: false, color: None, size: 14.0, el: self.el(), button: false, underline: false, input: NO_EL, fx: 0 });
                self.doc.blocks.push(blk);
            }
            self.start(Kind::P);
            self.link_text(&alloc::format!("▶ Video {}: (bosing — yuklab oçiş)", fname), link);
        } else {
            self.start(Kind::P);
            let play = if msrc.is_empty() { None } else { Some(alloc::format!("zamin-play:{}", msrc)) };
            self.link_text(&alloc::format!("▶ Ijro / ■ töxtatiş — ♪ {}", fname), play);
            self.text("  ");
            self.link_text("↓ yuklab oliş", link);
        }
        self.flush();
    }
    fn link_text(&mut self, text: &str, link: Option<String>) {
        if self.cur.is_none() {
            self.cur = Some(self.new_block(Kind::P));
        }
        let st = self.top();
        let el = self.el();
        if let Some(b) = self.cur.as_mut() {
            b.spans.push(Span { text: String::from(text), link, bold: true, code: false, br: false, color: st.color, size: st.size, el, button: false, underline: true, input: NO_EL, fx: 0 });
        }
    }
    fn br(&mut self) {
        if let Some(b) = self.cur.as_mut() {
            let size = b.size;
            b.spans.push(Span { text: String::new(), link: None, bold: false, code: false, br: true, color: None, size, el: NO_EL, button: false, underline: false, input: NO_EL, fx: 0 });
        }
    }
    fn open(&mut self, name: &str, tag: &str) {
        let mut id = attr(tag, "id").unwrap_or_default();
        if id.is_empty() && name == "a" {
            id = attr(tag, "name").unwrap_or_default(); // <a name="…"> — yakor
        }
        let classes: Vec<String> = attr(tag, "class").map(|c| c.split_whitespace().map(String::from).collect()).unwrap_or_default();
        let mut parent = self.top();
        // dir="rtl" / dir="ltr" — CSS `direction` dan oldin (CSS ustun turadi)
        match attr(tag, "dir").map(|d| d.to_ascii_lowercase()).as_deref() {
            Some("rtl") => parent.rtl = true,
            Some("ltr") => parent.rtl = false,
            _ => {}
        }
        let mut st = {
            let anc: Vec<ElRef> = self.stack.iter().map(|o| ElRef { tag: &o.tag, id: &o.id, classes: &o.classes, hover: false }).collect();
            let me = ElRef { tag: name, id: &id, classes: &classes, hover: false };
            let style = attr(tag, "style");
            css::compute(&parent, &self.rules, &me, &anc, style.as_deref())
        };
        // Eski HTML atributlari
        if let Some(c) = attr(tag, "bgcolor").and_then(|c| css::parse_color(&c)) {
            st.bg = Some(c);
        }
        if name == "font" {
            if let Some(c) = attr(tag, "color").and_then(|c| css::parse_color(&c)) {
                st.color = Some(c);
            }
            if let Some(sz) = attr(tag, "size") {
                const SIZES: [f32; 7] = [10.0, 13.0, 16.0, 18.0, 24.0, 32.0, 48.0];
                let t = sz.trim();
                let n: i32 = if let Some(r) = t.strip_prefix('+') { 3 + r.parse::<i32>().unwrap_or(0) } else if let Some(r) = t.strip_prefix('-') { 3 - r.parse::<i32>().unwrap_or(0) } else { t.parse().unwrap_or(3) };
                st.size = SIZES[(n.clamp(1, 7) - 1) as usize];
            }
        }
        // Jadval kataklari: border / cellpadding atributlari, colspan
        if name == "td" || name == "th" {
            if self.tborder.last().copied().unwrap_or(false) {
                st.bw = st.bw.max(1.0);
                st.bc = st.bc.or(Some(0x808080));
                st.pad = st.pad.max(4.0);
            }
            if let Some(c) = attr(tag, "colspan").and_then(|v| v.trim().parse::<f32>().ok()) {
                st.grow = c.clamp(1.0, 12.0);
            }
            if let Some(w) = attr(tag, "width") {
                if let Some(p) = w.strip_suffix('%') {
                    if let Ok(v) = p.trim().parse::<f32>() {
                        st.maxw = Some((v, true));
                    }
                } else if let Ok(v) = w.trim().parse::<f32>() {
                    st.maxw = Some((v, false));
                }
            }
        }
        if name == "table" || name == "img" {
            if let Some(w) = attr(tag, "width").filter(|w| w.ends_with('%')) {
                if let Ok(v) = w.trim_end_matches('%').trim().parse::<f32>() {
                    st.maxw = Some((v, true));
                }
            }
        }
        if let Some(a) = attr(tag, "align") {
            st.align = match a.to_ascii_lowercase().as_str() {
                "center" => 1,
                "right" => 2,
                "left" => 3,
                "justify" => 5,
                _ => st.align,
            };
        }
        if attr(tag, "hidden").is_some() || tag.split_whitespace().any(|w| w.eq_ignore_ascii_case("hidden")) {
            st.hidden = true;
        }
        let onclick = attr(tag, "onclick");
        // Har bir element ro'yxatga olinadi (JS: querySelectorAll, classList, hodisalar qalqişi)
        let el = if self.doc.elems.len() < 60_000 {
            let label_for = if name == "label" { attr(tag, "for").unwrap_or_default() } else { String::new() };
            self.doc.elems.push(ElInfo { id: id.clone(), tag: String::from(name), onclick, classes: classes.clone(), parent: self.el(), label_for, style: attr(tag, "style").unwrap_or_default() });
            (self.doc.elems.len() - 1) as u16
        } else {
            self.el()
        };
        if name == "body" || name == "html" {
            if st.bg.is_some() {
                self.doc.bg = st.bg;
            }
            if st.color.is_some() {
                self.doc.color = st.color;
            }
        }
        let link = if name == "a" { attr(tag, "href") } else { None };
        self.serial += 1;
        let serial = self.serial;
        let rspan = if name == "td" || name == "th" { attr(tag, "rowspan").and_then(|v| v.trim().parse::<u8>().ok()).unwrap_or(1).clamp(1, 64) } else { 1 };
        self.stack.push(Open { tag: String::from(name), id, classes, sty: st, el, link, serial, input: NO_EL, rspan });
    }
    /// rowspan: joriy qatorda yuqoridan çozilgan kataklar uçun bo'ş joy egallovçi kataklar.
    /// `all` — qator oxiri (qolgan hamma ustunlar), aks holda faqat joriy ustundagilar.
    fn rs_fill(&mut self, all: bool) {
        let Some(tr) = self.stack.iter().rposition(|o| o.tag == "tr") else { return };
        let Some(g) = self.tgrid.last_mut() else { return };
        if !g.in_row {
            return;
        }
        let mut out: Vec<(Option<(f32, bool)>, f32)> = Vec::new();
        loop {
            let (row, col) = (g.row, g.col);
            let hit = g.pend.iter_mut().filter(|p| p.left > 0 && p.row != row && (p.start == col || (all && p.start >= col))).min_by_key(|p| p.start);
            match hit {
                Some(p) => {
                    p.left -= 1;
                    p.row = row;
                    g.col = p.start + p.n;
                    out.push((p.fw, p.grow));
                }
                None => break,
            }
        }
        g.pend.retain(|p| p.left > 0);
        if out.is_empty() {
            return;
        }
        self.flush();
        let (tser, tsty) = (self.stack[tr].serial, self.stack[tr].sty);
        for (fw, grow) in out {
            self.serial += 1;
            let mut b = self.new_block(Kind::P);
            b.flex = tser;
            b.fcol = self.serial;
            b.fmode = tsty.flex;
            b.fgap = tsty.gap;
            b.fjust = tsty.justify;
            b.fw = fw;
            b.fgrow = grow;
            b.rspan = 0; // joy egallovçi
            (b.bg, b.bgk, b.pad, b.bw, b.bc, b.radius, b.maxw, b.center) = (None, 0, 0.0, 0.0, None, 0.0, None, false);
            (b.grad, b.shadow) = (None, None);
            b.spans.push(Span { text: String::from("\u{2007}"), link: None, bold: false, code: false, br: false, color: None, size: b.size, el: NO_EL, button: false, underline: false, input: NO_EL, fx: 0 });
            self.doc.blocks.push(b);
        }
    }
    /// Yangi katak: avval egallangan ustunlarni töldiradi, keyin rowspan ni yozib qöyadi.
    fn rs_cell(&mut self, tag: &str) {
        self.rs_fill(false);
        let Some(g) = self.tgrid.last_mut() else { return };
        if !g.in_row {
            return;
        }
        let n = attr(tag, "colspan").and_then(|v| v.trim().parse::<usize>().ok()).unwrap_or(1).clamp(1, 12);
        let r = attr(tag, "rowspan").and_then(|v| v.trim().parse::<u8>().ok()).unwrap_or(1).clamp(1, 64);
        if r > 1 {
            g.pend.push(Pend { start: g.col, n, left: r - 1, row: g.row, fw: None, grow: n as f32 });
        }
        g.col += n;
        if r > 1 {
            self.rs_new = true;
        }
    }
    fn close(&mut self, name: &str) {
        if let Some(k) = self.stack.iter().rposition(|o| o.tag == name) {
            self.stack.truncate(k);
        }
    }
}

/// Ro'yxat belgisi: 1, a, A, i, I turlari.
fn list_marker(n: i32, ty: char) -> String {
    let n = n.max(0);
    match ty {
        'a' | 'A' => {
            let mut v = n.max(1) as u32;
            let mut s = Vec::new();
            while v > 0 {
                v -= 1;
                s.push((b'a' + (v % 26) as u8) as char);
                v /= 26;
            }
            let r: String = s.into_iter().rev().collect();
            if ty == 'A' { r.to_uppercase() } else { r }
        }
        'i' | 'I' => {
            const R: [(i32, &str); 13] = [(1000, "m"), (900, "cm"), (500, "d"), (400, "cd"), (100, "c"), (90, "xc"), (50, "l"), (40, "xl"), (10, "x"), (9, "ix"), (5, "v"), (4, "iv"), (1, "i")];
            let mut v = n.clamp(1, 3999);
            let mut s = String::new();
            for (k, r) in R {
                while v >= k {
                    s.push_str(r);
                    v -= k;
                }
            }
            if ty == 'I' { s.to_uppercase() } else { s }
        }
        _ => alloc::format!("{}", n),
    }
}

/// srcset dagi birinçi manzil ("a.png 1x, b.png 2x" → "a.png").
fn first_srcset(v: &str) -> String {
    String::from(v.split(',').next().unwrap_or("").split_whitespace().next().unwrap_or(""))
}

/// Baytlarni matnga: UTF-8, windows-1251, latin-1 / windows-1252 (Content-Type yoki <meta charset>).
pub fn decode_bytes(b: &[u8], ctype: &str) -> String {
    let probe = String::from_utf8_lossy(&b[..b.len().min(2048)]).to_ascii_lowercase();
    let cs = ctype
        .to_ascii_lowercase()
        .split("charset=")
        .nth(1)
        .map(|c| String::from(c.trim_matches(|ch: char| ch == '"' || ch == '\'' || ch.is_whitespace())))
        .or_else(|| probe.split("charset=").nth(1).map(|c| c.trim_start_matches(['"', '\'']).chars().take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '-' || *ch == '_').collect()))
        .unwrap_or_default();
    match cs.as_str() {
        "windows-1251" | "cp1251" | "win-1251" => b.iter().map(|&c| cp1251(c)).collect(),
        "iso-8859-1" | "latin1" | "windows-1252" | "cp1252" | "us-ascii" | "ascii" => b
            .iter()
            .map(|&c| {
                const W: [u16; 32] = [
                    0x20ac, 0x81, 0x201a, 0x192, 0x201e, 0x2026, 0x2020, 0x2021, 0x2c6, 0x2030, 0x160, 0x2039, 0x152, 0x8d, 0x17d, 0x8f, 0x90, 0x2018,
                    0x2019, 0x201c, 0x201d, 0x2022, 0x2013, 0x2014, 0x2dc, 0x2122, 0x161, 0x203a, 0x153, 0x9d, 0x17e, 0x178,
                ];
                if (0x80..0xa0).contains(&c) { char::from_u32(W[(c - 0x80) as usize] as u32).unwrap_or('?') } else { c as char }
            })
            .collect(),
        _ => String::from_utf8_lossy(b).into_owned(),
    }
}

fn cp1251(c: u8) -> char {
    match c {
        0x00..=0x7f => c as char,
        0xc0..=0xff => char::from_u32(0x410 + (c - 0xc0) as u32).unwrap_or('?'),
        0xa8 => 'Ё',
        0xb8 => 'ё',
        0xa1 => 'Ў',
        0xa2 => 'ў',
        0xaa => 'Є',
        0xba => 'є',
        0xaf => 'Ї',
        0xbf => 'ї',
        0xb2 => 'І',
        0xb3 => 'і',
        0xa5 => 'Ґ',
        0xb4 => 'ґ',
        0x96 => '–',
        0x97 => '—',
        0x91 => '‘',
        0x92 => '’',
        0x93 => '“',
        0x94 => '”',
        0x85 => '…',
        0xab => '«',
        0xbb => '»',
        0xb9 => '№',
        0xa0 => '\u{a0}',
        0x88 => '€',
        _ => '?',
    }
}

/// <link rel=stylesheet href=...> manzillari.
pub fn stylesheet_links(src: &str) -> Vec<String> {
    let low = src.to_ascii_lowercase();
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(i) = low[from..].find("<link") {
        let st = from + i;
        let Some(e) = low[st..].find('>') else { break };
        let tag = &src[st + 1..st + e];
        if attr(tag, "rel").is_some_and(|r| r.to_ascii_lowercase().contains("stylesheet")) {
            if let Some(h) = attr(tag, "href") {
                out.push(h);
            }
        }
        from = st + e;
    }
    out
}

/// Hujjatdagi <style> mazmunlari.
/// Teg oxiri ('>' indeksi) — tirnoq içidagi '>' hisobga olinmaydi (`i` — '<' ning öçni).
fn tag_end_quoted(src: &str, i: usize) -> Option<usize> {
    let b = src.as_bytes();
    let mut q: Option<u8> = None;
    let mut prev = 0u8; // oxirgi böşliq bölmagan bayt
    for (k, &c) in b.iter().enumerate().skip(i + 1) {
        match q {
            Some(qc) if c == qc => q = None,
            Some(_) => {}
            None if (c == b'"' || c == b'\'') && prev == b'=' => q = Some(c),
            None if c == b'>' => return Some(k),
            None => {}
        }
        if !c.is_ascii_whitespace() {
            prev = c;
        }
    }
    None
}

/// `<iframe` teg boşimi (bayt indeksi `i` dagi '<')?
fn is_iframe_at(b: &[u8], i: usize) -> bool {
    b.get(i + 1..i + 7).is_some_and(|s| s.eq_ignore_ascii_case(b"iframe")) && b.get(i + 7).is_none_or(|c| !c.is_ascii_alphanumeric())
}

/// Teg matnidan `srcdoc="…"` atributini ajratadi: (qolgan teg, yeçilgan qiymat).
/// Qiymat içidagi `src=`, `style=` kabilar iframe ning özi atributlari deb öqilmasligi uçun.
pub fn split_srcdoc(tag: &str) -> (String, Option<String>) {
    let low = tag.to_ascii_lowercase();
    let lb = low.as_bytes();
    let mut from = 0;
    while let Some(p) = low[from..].find("srcdoc") {
        let i = from + p;
        let before_ok = i > 0 && lb[i - 1].is_ascii_whitespace();
        let mut k = i + 6;
        while k < lb.len() && lb[k].is_ascii_whitespace() {
            k += 1;
        }
        if before_ok && lb.get(k) == Some(&b'=') {
            k += 1;
            while k < lb.len() && lb[k].is_ascii_whitespace() {
                k += 1;
            }
            let (vs, ve, end) = match lb.get(k) {
                Some(&q) if q == b'"' || q == b'\'' => {
                    let e = low[k + 1..].find(q as char).map(|j| k + 1 + j).unwrap_or(lb.len());
                    (k + 1, e, (e + 1).min(lb.len()))
                }
                _ => {
                    let e = low[k..].find(|c: char| c.is_whitespace()).map(|j| k + j).unwrap_or(lb.len());
                    (k, e, e)
                }
            };
            let mut rest = String::from(&tag[..i]);
            rest.push(' ');
            rest.push_str(&tag[end..]);
            return (rest, Some(decode_entities(&tag[vs..ve])));
        }
        from = i + 6;
    }
    (String::from(tag), None)
}

/// HTML matnini `srcdoc` atributi qiymati sifatida xavfsiz yozadi.
pub fn escape_attr(s: &str) -> String {
    s.replace('&', "&amp;").replace('"', "&quot;").replace('<', "&lt;").replace('>', "&gt;")
}

/// Bir xil manbali `<iframe src>` lar: `fetch(src)` HTML qaytarsa, u `srcdoc` sifatida tegga
/// qöşiladi (keyin tahlilçi uni ichki hujjat sifatida chizadi; @media qayta tahlilida ham saqlanadi).
pub fn inline_iframes(src: &str, max: usize, mut fetch: impl FnMut(&str) -> Option<String>) -> String {
    let b = src.as_bytes();
    let mut out = String::with_capacity(src.len());
    let (mut last, mut i, mut n) = (0, 0, 0);
    while let Some(p) = src[i..].find('<') {
        let st = i + p;
        if src[st..].starts_with("<!--") {
            i = src[st..].find("-->").map(|j| st + j + 3).unwrap_or(b.len());
            continue;
        }
        if !is_iframe_at(b, st) {
            i = st + 1;
            continue;
        }
        let Some(end) = tag_end_quoted(src, st) else { break };
        let tag = &src[st + 1..end];
        let (_, sd) = split_srcdoc(tag);
        if sd.is_none() && n < max {
            if let Some(u) = attr(tag, "src").filter(|u| !u.trim().is_empty()) {
                n += 1;
                if let Some(html) = fetch(u.trim()) {
                    out.push_str(&src[last..end]);
                    out.push_str(" srcdoc=\"");
                    out.push_str(&escape_attr(&html));
                    out.push('"');
                    last = end;
                }
            }
        }
        i = end + 1;
    }
    out.push_str(&src[last..]);
    out
}

fn inline_css(src: &str) -> String {
    let low = src.to_ascii_lowercase();
    let mut out = String::new();
    let mut from = 0;
    while let Some(i) = low[from..].find("<style") {
        let st = from + i;
        // iframe srcdoc içidagi <style> — ota hujjatga tegişli emas
        if let Some(f) = low[from..st].rfind("<iframe").map(|j| from + j) {
            if let Some(e) = tag_end_quoted(src, f).filter(|&e| e > st) {
                from = e + 1;
                continue;
            }
        }
        let Some(gt) = low[st..].find('>') else { break };
        let body = st + gt + 1;
        let end = low[body..].find("</style").map(|j| body + j).unwrap_or(low.len());
        out.push_str(&src[body..end]);
        out.push('\n');
        from = end;
    }
    out
}

pub fn parse(src: &str) -> Doc {
    parse_with(src, "")
}

/// HTML + tashqi CSS (<link> orqali yuklangan) → hujjat.
pub fn parse_with(src: &str, extra_css: &str) -> Doc {
    parse_doc(src, extra_css, 0)
}

fn parse_doc(src: &str, extra_css: &str, depth: u8) -> Doc {
    let mut sheet = String::from(UA_CSS);
    sheet.push_str(extra_css);
    sheet.push_str(&inline_css(src));
    let rules = css::parse_sheet(&sheet);
    let mut d = parse_inner(src, &rules, true, depth);
    d.media_on = css::media_state(&rules);
    if !d.media_on.is_empty() {
        d.src = String::from(src);
        d.extra_css = String::from(extra_css);
    }
    d.rules = rules;
    d
}

fn parse_inner(src: &str, rules: &[Rule], implicit_body: bool, depth: u8) -> Doc {
    let rules: Vec<Rule> = rules.to_vec();
    let mut p = P { doc: Doc::empty(), rules, stack: Vec::new(), cur: None, pre: 0, lists: Vec::new(), in_title: false, serial: 0, form: NO_EL, dets: Vec::new(), tborder: Vec::new(), tgrid: Vec::new(), rs_new: false, media: None, pic_src: String::new(), depth };
    // Yaşirin <html> va <body> (teglar bölmasa ham uslublari qöllanadi)
    if implicit_body {
        p.open("html", "html");
        p.open("body", "body");
    }
    let mut i = 0;
    let b = src.as_bytes();
    while i < b.len() {
        if b[i] == b'<' {
            if src[i..].starts_with("<!--") {
                i = src[i..].find("-->").map(|j| i + j + 3).unwrap_or(b.len());
                continue;
            }
            let qend = if is_iframe_at(b, i) { tag_end_quoted(src, i) } else { None };
            let Some(end) = qend.or_else(|| src[i..].find('>').map(|j| i + j)) else { break };
            let tag = &src[i + 1..end];
            i = end + 1;
            let closing = tag.starts_with('/');
            let t = tag.trim_start_matches('/');
            let name: String = t.chars().take_while(|c| c.is_ascii_alphanumeric()).collect::<String>().to_ascii_lowercase();
            if name.is_empty() {
                continue;
            }
            // <iframe srcdoc="…">: qiymat ajratiladi, qolgan atributlar odatdagiday öqiladi
            let (t_own, srcdoc) = if !closing && name == "iframe" { split_srcdoc(t) } else { (String::new(), None) };
            let t: &str = if srcdoc.is_some() { &t_own } else { t };
            if !closing && name == "svg" {
                // Ichki SVG — butunligiça olinadi, keyin SVG dekoderimiz bilan rasmga aylanadi
                let low = src[i..].to_ascii_lowercase();
                let e = low.find("</svg").map(|j| i + j).unwrap_or(b.len());
                let close_end = src[e..].find('>').map(|j| e + j + 1).unwrap_or(b.len());
                let mut markup = alloc::format!("<{}>", tag);
                markup.push_str(&src[i..e]);
                markup.push_str("</svg>");
                if !p.hidden() {
                    p.flush();
                    let n = p.doc.svgs.len();
                    p.doc.svgs.push(markup);
                    let mut blk = p.new_block(Kind::Img);
                    blk.src = alloc::format!("svg:{}", n);
                    blk.iw = attr(t, "width").and_then(|v| v.trim_end_matches("px").trim().parse().ok()).unwrap_or(0);
                    blk.ih = attr(t, "height").and_then(|v| v.trim_end_matches("px").trim().parse().ok()).unwrap_or(0);
                    let link = p.stack.iter().rev().find_map(|o| o.link.clone());
                    blk.spans.push(Span { text: String::from("svg"), link, bold: false, code: false, br: false, color: None, size: 14.0, el: p.el(), button: false, underline: false, input: NO_EL, fx: 0 });
                    p.doc.blocks.push(blk);
                }
                i = close_end;
                continue;
            }
            if !closing && name == "select" {
                // <option value>yozuv</option> lar; tanlangani (selected) yoki birinçisi
                let low = src[i..].to_ascii_lowercase();
                let e = low.find("</select").map(|j| i + j).unwrap_or(b.len());
                let inner = &src[i..e];
                let mut opts: Vec<(String, String)> = Vec::new();
                let mut sel = 0;
                let mut from = 0;
                let il = inner.to_ascii_lowercase();
                while let Some(k) = il[from..].find("<option") {
                    let st = from + k;
                    let Some(gt) = inner[st..].find('>').map(|g| st + g) else { break };
                    let otag = &inner[st + 1..gt];
                    let end = il[gt..].find("</option").map(|x| gt + x).or_else(|| il[gt + 1..].find("<option").map(|x| gt + 1 + x)).unwrap_or(inner.len());
                    let label = decode_entities(strip_tags(&inner[gt + 1..end]).trim());
                    let value = attr(otag, "value").unwrap_or_else(|| label.clone());
                    if otag.to_ascii_lowercase().contains("selected") {
                        sel = opts.len();
                    }
                    opts.push((value, label));
                    from = end;
                }
                let v = opts.get(sel).map(|o| o.0.clone()).unwrap_or_default();
                let k = p.add_input(6, t, v, None);
                let w = opts.iter().map(|o| o.1.chars().count()).max().unwrap_or(4) as u16 + 6;
                p.doc.inputs[k as usize].size = w.clamp(6, 40);
                p.doc.inputs[k as usize].options = opts;
                p.input_span(k);
                i = e;
                continue;
            }
            if !closing && name == "textarea" {
                let low = src[i..].to_ascii_lowercase();
                let e = low.find("</textarea").map(|j| i + j).unwrap_or(b.len());
                let v = decode_entities(&src[i..e]);
                let k = p.add_input(0, t, v, None);
                p.doc.inputs[k as usize].size = attr(t, "cols").and_then(|v| v.parse().ok()).unwrap_or(40);
                p.input_span(k);
                i = e;
                continue;
            }
            if !closing && (name == "script" || name == "style" || name == "noscript" || name == "template") {
                let close = alloc::format!("</{}", name);
                let low = src[i..].to_ascii_lowercase();
                let e = low.find(&close).map(|j| i + j).unwrap_or(b.len());
                if name == "script" {
                    match attr(t, "src") {
                        Some(u) => p.doc.scripts.push(Script::Src(u)),
                        None => p.doc.scripts.push(Script::Inline(String::from(&src[i..e]))),
                    }
                }
                i = e;
                continue;
            }
            if closing && (name == "body" || name == "html") {
                continue; // yaşirin elementlar oxirigaça qoladi
            }
            if !closing && (name == "body" || name == "html") {
                // Haqiqiy teg: atributlari bilan uslubni qayta hisoblaymiz (joyida)
                if let Some(k) = p.stack.iter().position(|o| o.tag == name) {
                    let rest = p.stack.split_off(k);
                    p.open(&name, t);
                    p.stack.extend(rest.into_iter().skip(1));
                }
                p.flush();
                continue;
            }
            if closing {
                match name.as_str() {
                    "video" | "audio" => {
                        p.close(&name);
                        if let Some((kind, msrc, poster, mw, mh)) = p.media.take() {
                            if !p.hidden() {
                                p.media_card(&kind, msrc, poster, mw, mh);
                            }
                        }
                        continue;
                    }
                    "q" => p.text("”"),
                    "details" => {
                        p.flush();
                        p.dets.pop();
                    }
                    "summary" | "caption" | "legend" | "figcaption" => p.flush(),
                    "table" => {
                        p.rs_fill(true);
                        p.flush();
                        p.tborder.pop();
                        p.tgrid.pop();
                    }
                    "tr" => {
                        p.rs_fill(true);
                        p.flush();
                        if let Some(g) = p.tgrid.last_mut() {
                            g.in_row = false;
                        }
                    }
                    "form" => {
                        p.flush();
                        p.form = NO_EL;
                    }
                    "title" => p.in_title = false,
                    "p" | "div" | "section" | "article" | "header" | "footer" | "nav" | "main" | "td" | "th" | "center" | "aside" | "figure" | "dd" | "dt" | "dl" | "fieldset" | "address" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "li" | "blockquote" => p.flush(),
                    "ul" | "ol" => {
                        p.flush();
                        p.lists.pop();
                    }
                    "pre" => {
                        p.pre = p.pre.saturating_sub(1);
                        p.flush();
                    }
                    _ => {}
                }
                p.close(&name);
                continue;
            }
            // Yopilmagan <p>/<li> — yangisi boşlanganda yopamiz
            if (name == "p" || name == "li") && p.stack.last().is_some_and(|o| o.tag == name) {
                p.stack.pop();
            }
            let void = VOID.contains(&name.as_str()) || t.ends_with('/');
            // Jadval tarmoği (rowspan): qator / katak boşlanişi
            match name.as_str() {
                "tr" => {
                    p.rs_fill(true);
                    if let Some(g) = p.tgrid.last_mut() {
                        g.row += 1;
                        g.col = 0;
                        g.in_row = true;
                    }
                }
                "td" | "th" => p.rs_cell(t),
                _ => {}
            }
            if !void {
                p.open(&name, t);
            }
            if p.rs_new {
                p.rs_new = false;
                let fw = p.top().maxw;
                if let Some(pd) = p.tgrid.last_mut().and_then(|g| g.pend.last_mut()) {
                    pd.fw = fw;
                }
            }
            let blockish = p.top().block == Some(true);
            match name.as_str() {
                "title" => p.in_title = true,
                "h1" => p.start(Kind::H(1)),
                "h2" => p.start(Kind::H(2)),
                "h3" | "h4" | "h5" | "h6" => p.start(Kind::H(3)),
                "p" | "div" | "section" | "article" | "header" | "footer" | "nav" | "main" | "tr" | "center" | "aside" | "figure" | "body" => p.flush(),
                "br" => p.br(),
                "hr" => {
                    p.start(Kind::Hr);
                    p.flush();
                }
                "ul" | "menu" => {
                    p.flush();
                    p.lists.push((false, 0, '1', false));
                }
                "ol" => {
                    p.flush();
                    let ty = attr(t, "type").and_then(|v| v.chars().next()).unwrap_or('1');
                    let rev = attr(t, "reversed").is_some() || t.split_whitespace().any(|w| w.eq_ignore_ascii_case("reversed"));
                    let start: i32 = match attr(t, "start").and_then(|v| v.trim().parse().ok()) {
                        Some(v) => v,
                        None if rev => {
                            // teskari: elementlar sonidan boşlanadi
                            let low = src[i..].to_ascii_lowercase();
                            let end = low.find("</ol").unwrap_or(low.len());
                            low[..end].matches("<li").count() as i32
                        }
                        None => 1,
                    };
                    p.lists.push((true, if rev { start + 1 } else { start - 1 }, ty, rev));
                }
                "li" => {
                    let depth = p.lists.len().max(1) as u8;
                    let force = attr(t, "value").and_then(|v| v.trim().parse::<i32>().ok());
                    let (n, mark) = match p.lists.last_mut() {
                        Some((true, c, ty, rev)) => {
                            *c = match force {
                                Some(v) => v,
                                None if *rev => *c - 1,
                                None => *c + 1,
                            };
                            ((*c).max(1) as u16, alloc::format!("{}.", list_marker(*c, *ty)))
                        }
                        _ => (0, String::new()),
                    };
                    p.start(Kind::Li(depth, n));
                    if let Some(b) = p.cur.as_mut() {
                        b.mark = mark;
                    }
                }
                "dt" | "summary" | "caption" | "legend" | "figcaption" | "address" | "fieldset" => p.start(Kind::P),
                "dd" => p.start(Kind::Indent(1)),
                "details" => {
                    p.flush();
                    let open = attr(t, "open").is_some() || t.split_whitespace().any(|w| w.eq_ignore_ascii_case("open"));
                    p.doc.details.push(open);
                    p.dets.push(((p.doc.details.len() - 1) as u16, open));
                }
                "table" => {
                    p.flush();
                    let bd = attr(t, "border").is_some_and(|v| v.trim() != "0") || t.split_whitespace().any(|w| w.eq_ignore_ascii_case("border"));
                    p.tborder.push(bd);
                    p.tgrid.push(TGrid::default());
                }
                "q" => p.text("“"),
                "base" => {
                    if let Some(h) = attr(t, "href") {
                        p.doc.base = Some(h);
                    }
                }
                "meta" => {
                    if attr(t, "http-equiv").is_some_and(|v| v.eq_ignore_ascii_case("refresh")) {
                        if let Some(c) = attr(t, "content") {
                            let (secs, rest) = c.split_once(';').unwrap_or((&c, ""));
                            let url = rest.trim().trim_start_matches(|ch: char| ch.is_ascii_alphabetic() || ch == '=').trim_matches(|ch| ch == '\'' || ch == '"');
                            let url = rest.trim().splitn(2, '=').nth(1).map(|u| u.trim().trim_matches(|ch| ch == '\'' || ch == '"')).unwrap_or(url);
                            p.doc.refresh = Some((secs.trim().parse().unwrap_or(0), String::from(url)));
                        }
                    }
                }
                "source" => {
                    let srcv = attr(t, "srcset").map(|v| first_srcset(&v)).or_else(|| attr(t, "src"));
                    if let Some(sv) = srcv {
                        if let Some(m) = p.media.as_mut() {
                            // birinçi qöllanadigan manba (audio uçun WAV afzal)
                            let wav = m.0 == "audio" && (sv.to_ascii_lowercase().split('?').next().unwrap_or("").ends_with(".wav") || attr(t, "type").is_some_and(|v| v.contains("wav")));
                            if m.1.is_empty() || wav && !m.1.to_ascii_lowercase().contains(".wav") {
                                m.1 = sv;
                            }
                        } else if p.pic_src.is_empty() && p.stack.iter().any(|o| o.tag == "picture") {
                            p.pic_src = sv;
                        }
                    }
                }
                "video" | "audio" => {
                    let num = |k: &str| attr(t, k).and_then(|v| v.trim().trim_end_matches("px").trim().parse().ok()).unwrap_or(0);
                    p.media = Some((name.clone(), attr(t, "src").unwrap_or_default(), attr(t, "poster").unwrap_or_default(), num("width"), num("height")));
                    if let Some(o) = p.stack.last_mut() {
                        o.sty.hidden = true; // içidagi zaxira matn körinmasin
                    }
                }
                "iframe" if srcdoc.is_some() => {
                    let fel = p.stack.last().filter(|o| o.tag == "iframe").map(|o| o.el).unwrap_or(NO_EL);
                    let hidden = p.stack.len() >= 2 && p.stack[p.stack.len() - 2].sty.hidden;
                    if !hidden {
                        p.embed_frame(srcdoc.as_deref().unwrap_or(""), t, fel);
                    }
                    if let Some(o) = p.stack.last_mut().filter(|o| o.tag == name) {
                        o.sty.hidden = true; // zaxira matn körinmasin
                    }
                }
                "iframe" | "embed" | "object" => {
                    let u = attr(t, "src").or_else(|| attr(t, "data")).unwrap_or_default();
                    if !u.is_empty() {
                        p.flush();
                        // kartoçka ota element kontekstida (iframe ning özi yaşirin bölgunça)
                        let saved = p.stack.last().map(|o| o.sty.hidden);
                        if let Some(o) = p.stack.last_mut() {
                            o.sty.hidden = false;
                        }
                        p.start(Kind::P);
                        let label = alloc::format!("▣ Ichki sahifa: {}", u);
                        p.link_text(&label, Some(u));
                        p.flush();
                        if let (Some(o), Some(h)) = (p.stack.last_mut(), saved) {
                            o.sty.hidden = h;
                        }
                    }
                    if let Some(o) = p.stack.last_mut().filter(|o| o.tag == name) {
                        o.sty.hidden = true;
                    }
                }
                "canvas" => {
                    p.flush();
                    let mut blk = p.new_block(Kind::Img);
                    blk.iw = attr(t, "width").and_then(|v| v.trim().parse().ok()).unwrap_or(300);
                    blk.ih = attr(t, "height").and_then(|v| v.trim().parse().ok()).unwrap_or(150);
                    blk.spans.push(Span { text: String::from("canvas"), link: None, bold: false, code: false, br: false, color: None, size: 14.0, el: p.el(), button: false, underline: false, input: NO_EL, fx: 0 });
                    if p.doc.canvases.len() < 16 {
                        blk.src = alloc::format!("canvas:{}", p.doc.canvases.len());
                        let cv = crate::canvas::Canvas::new(p.el(), blk.iw as usize, blk.ih as usize);
                        p.doc.canvases.push(cv);
                    }
                    p.doc.blocks.push(blk);
                    if let Some(o) = p.stack.last_mut() {
                        o.sty.hidden = true;
                    }
                }
                "progress" | "meter" => {
                    let v = attr(t, "value").unwrap_or_default();
                    let k = p.add_input(9, t, v, None);
                    p.doc.inputs[k as usize].form = NO_EL;
                    p.input_span(k);
                    if let Some(o) = p.stack.last_mut() {
                        o.sty.hidden = true;
                    }
                }
                "pre" => {
                    p.start(Kind::Pre);
                    p.pre += 1;
                }
                "blockquote" => p.start(Kind::Quote),
                "img" => {
                    if !p.hidden() {
                        let mut src_url = attr(t, "src").unwrap_or_default();
                        if src_url.is_empty() {
                            src_url = attr(t, "srcset").map(|v| first_srcset(&v)).unwrap_or_else(|| p.pic_src.clone());
                        }
                        // Lazy-load atributlari (data-src)
                        if src_url.is_empty() || src_url.starts_with("data:image/gif;base64,R0lGOD") {
                            if let Some(d) = attr(t, "data-src").or_else(|| attr(t, "data-original")) {
                                src_url = d;
                            }
                        }
                        p.pic_src.clear();
                        let alt = attr(t, "alt").unwrap_or_default();
                        p.flush();
                        let mut blk = p.new_block(Kind::Img);
                        blk.src = src_url;
                        blk.iw = attr(t, "width").and_then(|v| v.trim_end_matches("px").trim().parse().ok()).unwrap_or(0);
                        blk.ih = attr(t, "height").and_then(|v| v.trim_end_matches("px").trim().parse().ok()).unwrap_or(0);
                        blk.spans.push(Span { text: alt, link: p.stack.iter().rev().find_map(|o| o.link.clone()), bold: false, code: false, br: false, color: None, size: 14.0, el: p.el(), button: false, underline: false, input: NO_EL, fx: 0 });
                        p.void_pos(&mut blk, "img", t);
                        p.doc.blocks.push(blk);
                    }
                }
                "td" | "th" => p.flush(),
                "input" => {
                    let ty = attr(t, "type").unwrap_or_default().to_ascii_lowercase();
                    let kind = match ty.as_str() {
                        "submit" | "button" | "reset" | "image" | "file" => 1,
                        "checkbox" | "radio" => 2,
                        "password" => 3,
                        "hidden" => 4,
                        "color" => 7,
                        "range" => 8,
                        _ => 0, // text, email, number, search, url, tel, date, time…
                    };
                    let mut v = attr(t, "value").unwrap_or_default();
                    if kind == 1 && v.is_empty() {
                        v = String::from(match ty.as_str() {
                            "reset" => "Tozalaş",
                            "file" => "Fayl tanlaş…",
                            _ => "Yuboriş",
                        });
                    }
                    if kind == 7 && v.is_empty() {
                        v = String::from("#000000");
                    }
                    let k = p.add_input(kind, t, v, None);
                    let inp = &mut p.doc.inputs[k as usize];
                    if ty == "button" || ty == "file" {
                        inp.form = NO_EL; // oddiy tugma — formani yubormaydi
                    }
                    if ty == "radio" {
                        inp.radio = true;
                        if inp.value.is_empty() {
                            inp.value = String::from("on");
                        }
                    }
                    if kind == 8 && inp.value.is_empty() {
                        inp.value = alloc::format!("{}", (inp.min + inp.max) / 2.0);
                    }
                    if kind == 0 && inp.placeholder.is_empty() {
                        inp.placeholder = String::from(match ty.as_str() {
                            "email" => "nom@pochta.uz",
                            "date" => "kk.oo.yyyy",
                            "time" => "ss:dd",
                            "url" => "https://",
                            "tel" => "+998 …",
                            "number" => "0",
                            "search" => "Qidiriş…",
                            _ => "",
                        });
                    }
                    p.input_span(k);
                }
                "form" => {
                    p.flush();
                    let action = attr(t, "action").unwrap_or_default();
                    let method = attr(t, "method").unwrap_or_else(|| String::from("get")).to_ascii_lowercase();
                    let el = p.stack.last().filter(|o| o.tag == "form").map(|o| o.el).unwrap_or(NO_EL);
                    p.doc.forms.push(Form { action, method, el });
                    p.form = (p.doc.forms.len() - 1) as u16;
                }
                "button" => {
                    let ty = attr(t, "type").unwrap_or_default().to_ascii_lowercase();
                    if p.form != NO_EL && ty != "button" {
                        // id/onclick open() da ro'yxatga olingan — öşa elementni işlatamiz
                        let el = p.stack.last().map(|o| o.el).unwrap_or(NO_EL);
                        let k = p.add_input(5, t, attr(t, "value").unwrap_or_default(), Some(el));
                        if let Some(o) = p.stack.last_mut() {
                            o.input = k;
                        }
                    }
                }
                _ if blockish => p.flush(),
                _ => {}
            }
            continue;
        }
        let next = src[i..].find('<').map(|j| i + j).unwrap_or(b.len());
        let t = &src[i..next];
        p.text(t);
        i = next;
    }
    p.flush();
    p.doc.title = String::from(p.doc.title.trim());
    p.doc
}

/// Teglarsiz matn (innerHTML qator içida).
fn strip_tags(s: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    decode_entities(&out)
}

/// HTML bölagi (innerHTML / appendChild) — hujjatning uslub qoidalari bilan.
fn parse_frag(src: &str, rules: &[Rule]) -> Doc {
    let mut d = parse_inner(src, rules, false, 0);
    d.title.clear();
    d
}

/// Oddiy matnni hujjatga (bitta <pre>) aylantiradi.
pub fn plain(text: &str) -> Doc {
    let mut d = Doc::empty();
    d.blocks.push(Block {
        kind: Kind::Pre,
        spans: alloc::vec![Span { text: String::from(text), link: None, bold: false, code: true, br: false, color: None, size: 14.0, el: NO_EL, button: false, underline: false, input: NO_EL, fx: 0 }],
        bg: None,
        align: 0,
        rtl: false,
        size: 14.0,
        color: None,
        el: NO_EL,
        src: String::new(),
        hidden: false,
        bgk: 0,
        pad: 0.0, bw: 0.0, bc: None, radius: 0.0, grad: None, shadow: None, maxw: None, center: false, mt: 0.0, mb: 0.0, bmt: 0.0, bmb: 0.0,
        flex: 0, fcol: 0, fmode: 0, fgap: 0.0, gcols: 0, fjust: 0, fw: None, fgrow: 0.0,
        mark: String::new(), det: NO_EL, summary: false, iw: 0, ih: 0, rspan: 1,
        oser: 0, okind: 0, ow: None, oins: [None; 4], pk: 0, aser: 0, rdx: 0.0, rdy: 0.0, bdx: 0.0, bdy: 0.0, clear: 0,
    });
    d
}

/// Namunaviy mahalliy sahifa (~/Hujjatlar/zamin.html).
pub const SAMPLE: &str = r#"<!doctype html>
<html><head><title>Zamin OS — qöllanma</title></head>
<body>
<h1>Zamin OS qöllanmasi</h1>
<p>Bu sahifa <b>Zamin Brauzer</b>da öz HTML tahlilçimiz bilan körsatilmoqda.
Fayl: <code>~/Hujjatlar/zamin.html</code>.</p>
<h2>Klaviatura yorliqlari</h2>
<ul>
<li><b>Alt+Space</b> — tezkor qidiruv</li>
<li><b>Alt+Tab</b> — oynalarni almaştiriş</li>
<li><b>PrtSc</b> — ekran rasmi</li>
<li><b>AltGr</b> + s / c / o / g — ş ç ö ğ</li>
</ul>
<h2>Ilovalar</h2>
<ol>
<li>Fayllar — sudrab tashlaş, savat, nusxa olish</li>
<li>Kalkulyator va Tizim monitori</li>
<li>Chizma — rasm chiziş va PNG saqlaş</li>
</ol>
<blockquote>Hammasi Rust tilida noldan yozilgan — tayyor kutubxonalarsiz.</blockquote>
<h3>Kod namunasi</h3>
<pre>fn main() {
    println!("Salom, Zamin!");
}</pre>
<hr>
<p>Havolalar: <a href="zamin://bosh">Bosh sahifa</a> · <a href="zamin://haqida">Brauzer haqida</a></p>
</body></html>
"#;

/// Jadval rowspan tekşiruvi: çozilgan kataklar pastki qatorlarda joy egallovçi oladi.
pub fn self_test_rowspan() -> Result<usize, &'static str> {
    let d = parse("<table border=1><tr><td rowspan=2>A</td><td>B</td></tr><tr><td>C</td></tr><tr><td>D</td><td rowspan=2>E</td><td>F</td></tr><tr><td>G</td><td>H</td></tr></table>");
    let txt = |b: &Block| b.spans.iter().map(|s| s.text.as_str()).collect::<String>();
    let seq: Vec<String> = d.blocks.iter().map(|b| if b.rspan == 0 { String::from("_") } else { txt(b) }).collect();
    let got = seq.join(",");
    if got != "A,B,_,C,D,E,F,G,_,H" {
        return Err("rowspan: kataklar tartibi notöğri");
    }
    if d.blocks[0].rspan != 2 || d.blocks[5].rspan != 2 || d.blocks[2].flex != d.blocks[3].flex || d.blocks[2].fcol == d.blocks[3].fcol {
        return Err("rowspan: qator/ustun bog'lanişi notöğri");
    }
    Ok(seq.len())
}

/// CSS position / float tekşiruvi: oqimdan taşqari elementlar va relative siljişlar.
pub fn self_test_position() -> Result<usize, &'static str> {
    let d = parse("<style>.r{position:relative;top:4px;left:-2px}.a{position:absolute;right:0;top:10px;width:50%}</style>\
        <div class=r><p>R</p><div class=a><p>A</p></div></div>\
        <div style=\"float:left;width:100px\"><p>L</p></div><p style=\"clear:both\">C</p><img src=x.png style=\"float:right\">");
    let find = |t: &str| d.blocks.iter().find(|b| b.spans.iter().any(|s| s.text.trim() == t));
    let r = find("R").ok_or("position: R bloki yöq")?;
    if r.rdx != -2.0 || r.rdy != 4.0 || r.oser != 0 || r.aser == 0 {
        return Err("position: relative siljişi notöğri");
    }
    let a = find("A").ok_or("position: A bloki yöq")?;
    if a.okind != 3 || a.pk != r.aser || a.oins[1] != Some(0.0) || a.oins[0] != Some(10.0) || a.ow != Some((50.0, true)) {
        return Err("position: absolute notöğri");
    }
    let l = find("L").ok_or("float: L bloki yöq")?;
    if l.okind != 1 || l.ow != Some((100.0, false)) {
        return Err("float: left notöğri");
    }
    let c = find("C").ok_or("clear: C bloki yöq")?;
    if c.clear != 3 || c.oser != 0 {
        return Err("clear: both notöğri");
    }
    let img = d.blocks.iter().find(|b| b.kind == Kind::Img).ok_or("float: rasm yöq")?;
    if img.okind != 2 {
        return Err("float: rasm (right) notöğri");
    }
    Ok(5)
}

/// dir="rtl" / direction va text-align (start/end/left/justify) tekşiruvi.
pub fn self_test_dir() -> Result<usize, &'static str> {
    let d = parse("<style>.e{text-align:end}.j{text-align:justify}.l{direction:ltr}</style>\
        <div dir=rtl><p>A</p><p class=e>B</p><p style=\"text-align:left\">C</p><p class=l>D</p><p class=j>E</p></div>\
        <p>F</p><p class=e>G</p><p align=center>H</p><p style=\"direction:rtl;text-align:start\">I</p>\
        <div style=\"text-align:right\"><p dir=ltr>J</p></div><ul dir=rtl><li>K</li></ul>");
    let find = |t: &str| d.blocks.iter().find(|b| b.spans.iter().any(|s| s.text.trim() == t)).map(|b| (b.align, b.rtl));
    let want: [(&str, u8, bool); 11] = [
        ("A", 2, true), ("B", 0, true), ("C", 0, true), ("D", 0, false), ("E", 3, true),
        ("F", 0, false), ("G", 2, false), ("H", 1, false), ("I", 2, true), ("J", 2, false), ("K", 2, true),
    ];
    for (t, a, r) in want {
        if find(t) != Some((a, r)) {
            return Err("dir/text-align: blok tekislaşi notöğri");
        }
    }
    let mut d = d;
    let el = d.blocks.iter().find(|b| b.spans.iter().any(|s| s.text.trim() == "F")).map(|b| b.el).ok_or("dir: F yöq")?;
    d.set_style(el as usize, "textAlign", "end");
    if find_in(&d, "F") != Some(2) {
        return Err("dir: JS style.textAlign qöllanmadi");
    }
    Ok(want.len() + 1)
}

fn find_in(d: &Doc, t: &str) -> Option<u8> {
    d.blocks.iter().find(|b| b.spans.iter().any(|s| s.text.trim() == t)).map(|b| b.align)
}

/// <iframe>: srcdoc ichki hujjati alohida CSS bilan ramkada chiziladi; bir xil manbali src yuklanadi.
/// <audio>/<video>: ijro havolasi (WAV manbasi afzal), video poster rasmi, zaxira matn yaşirin.
pub fn self_test_av() -> Result<usize, &'static str> {
    let d = parse("<p>A</p><audio controls><source src=k.mp3 type=audio/mpeg><source src=\"s/n.wav\" type=audio/wav>zaxira1</audio>\
        <video src=v.avi poster=\"p.png\" width=320 height=180>zaxira2</video><video src=w.mp4></video><p id=z>Z</p>");
    let links: Vec<&str> = d.blocks.iter().flat_map(|b| b.spans.iter()).filter_map(|s| s.link.as_deref()).collect();
    if !links.contains(&"zamin-play:s/n.wav") {
        return Err("audio: WAV ijro havolasi yöq");
    }
    if !links.contains(&"s/n.wav") || !links.contains(&"w.mp4") {
        return Err("audio/video: yuklab oliş havolasi yöq");
    }
    let posters: Vec<&Block> = d.blocks.iter().filter(|b| b.kind == Kind::Img).collect();
    if posters.len() != 1 || posters[0].src != "p.png" || posters[0].iw != 320 || posters[0].ih != 180 || posters[0].spans.first().and_then(|s| s.link.as_deref()) != Some("v.avi") {
        return Err("video: poster rasmi notöğri");
    }
    if d.blocks.iter().any(|b| !b.hidden && b.spans.iter().any(|s| s.text.contains("zaxira"))) {
        return Err("audio/video: zaxira matn körindi");
    }
    if d.find_id("z").is_none() {
        return Err("audio/video: keyingi element yöq");
    }
    Ok(links.len() + posters.len())
}

pub fn self_test_iframe() -> Result<usize, &'static str> {
    let blk = |d: &Doc, t: &str| d.blocks.iter().position(|b| b.spans.iter().any(|s| s.text.trim() == t));
    let src = "<p>Oldin</p><iframe width=\"300\" srcdoc=\"<style>p{color:#ff0000} body{background:#fafafa}</style><p class=x id=ich onclick='y()'>Ichki <b>matn</b></p><img src='r.png' alt=rasm>\" \
        src=\"tashqi.html\"><p>zaxira</p></iframe><p id=keyin>Keyin</p>";
    let d = parse(src);
    let i = blk(&d, "Ichki").ok_or("iframe: srcdoc matni chizilmadi")?;
    let b = &d.blocks[i];
    if b.spans.first().and_then(|s| s.color) != Some(0xff0000) && b.color != Some(0xff0000) {
        return Err("iframe: ichki <style> qöllanmadi");
    }
    if b.bgk == 0 || b.bg != Some(0xfafafa) || b.maxw != Some((300.0, false)) || b.bw <= 0.0 {
        return Err("iframe: ramka (fon / kenglik / çegara) notöğri");
    }
    let o = &d.blocks[blk(&d, "Oldin").ok_or("iframe: oldingi matn yöq")?];
    if o.color == Some(0xff0000) || o.spans.iter().any(|s| s.color == Some(0xff0000)) {
        return Err("iframe: ichki CSS ota hujjatga oqib çiqdi");
    }
    let k = &d.blocks[blk(&d, "Keyin").ok_or("iframe: keyingi matn yöq")?];
    if k.bgk == b.bgk || blk(&d, "zaxira").is_some_and(|z| !d.blocks[z].hidden) {
        return Err("iframe: ramka yopilmadi yoki zaxira matn körindi");
    }
    if d.find_id("ich").is_some() || d.find_id("keyin").is_none() || d.elems.iter().any(|e| e.onclick.as_deref() == Some("y()")) {
        return Err("iframe: ichki elementlar ota JS dan ajratilmadi");
    }
    if !d.blocks.iter().any(|x| x.kind == Kind::Img && x.src == "r.png" && x.bgk == b.bgk) {
        return Err("iframe: ichki rasm yöq");
    }
    if d.blocks.iter().flat_map(|x| x.spans.iter()).any(|s| s.text.contains("Ichki sahifa")) {
        return Err("iframe: srcdoc bilan ham havola-kartoçka chiqdi");
    }
    // Belgilar bilan kodlangan srcdoc va ichma-ich iframe (çuqurlik çegarasi)
    let d = parse("<iframe srcdoc=\"&lt;h1&gt;Sarlavha&lt;/h1&gt;&lt;iframe srcdoc='&amp;lt;p&amp;gt;Chuqur&amp;lt;/p&amp;gt;'&gt;&lt;/iframe&gt;\"></iframe>");
    let h = blk(&d, "Sarlavha").ok_or("iframe: kodlangan srcdoc yeçilmadi")?;
    if d.blocks[h].kind != Kind::H(1) || blk(&d, "Chuqur").is_none() {
        return Err("iframe: ichki sarlavha / ichma-ich iframe chizilmadi");
    }
    // Bir xil manbali src → srcdoc (yuklaş qayta çaqiruv orqali)
    let mut calls = 0;
    let t = inline_iframes("<iframe src=\"a.html\" width=50%></iframe><iframe src=\"b.html\" srcdoc=\"x\"></iframe>", 4, |u| {
        calls += 1;
        (u == "a.html").then(|| String::from("<p>\"Q\" &amp; R</p>"))
    });
    let d = parse(&t);
    let q = blk(&d, "\"Q\" & R").ok_or("iframe: yuklangan src chizilmadi")?;
    if calls != 1 || d.blocks[q].maxw != Some((50.0, true)) {
        return Err("iframe: src yuklaş / kenglik notöğri");
    }
    // Boşqa manbali — havola-kartoçka qoladi
    let d = parse("<iframe src=\"https://example.com/\"></iframe>");
    if !d.blocks.iter().flat_map(|x| x.spans.iter()).any(|s| s.text.contains("Ichki sahifa") && s.link.is_some()) {
        return Err("iframe: boşqa manbali havola-kartoçka yöqoldi");
    }
    Ok(8)
}

/// CSS @media va :hover tekşiruvi: kö'riş maydoni eniga qarab qoidalar, siçqonça ostidagi element uslubi.
pub fn self_test_media() -> Result<usize, &'static str> {
    let saved = css::viewport();
    let r = media_checks();
    css::set_viewport(saved.0, saved.1);
    r
}

fn media_checks() -> Result<usize, &'static str> {
    let src = "<style>.a{color:#000000}@media (max-width:600px){.a{color:#ff0000}}\
        @media screen and (min-width: 800px){.a{color:#00ff00}}@media print{.a{display:none}}\
        a{text-decoration:none}a:hover{color:#0000ff;text-decoration:underline}.k:hover .c{color:#123456}</style>\
        <p class=a>A</p><p><a href=x>L</a></p><div class=k><p class=c>C</p></div>";
    let col = |d: &Doc, t: &str| d.blocks.iter().flat_map(|b| b.spans.iter()).find(|s| s.text.trim() == t).map(|s| s.color);
    css::set_viewport(500.0, 700.0);
    let d = parse(src);
    if d.media_on.len() != 3 || col(&d, "A") != Some(Some(0xff0000)) {
        return Err("@media: max-width qoidasi qöllanmadi");
    }
    css::set_viewport(1000.0, 700.0);
    if css::media_state(&d.rules) == d.media_on {
        return Err("@media: en özgarişi sezilmadi");
    }
    let mut d = parse_with(&d.src, &d.extra_css);
    if col(&d, "A") != Some(Some(0x00ff00)) || d.blocks.iter().any(|b| b.hidden) {
        return Err("@media: min-width / print notöğri");
    }
    let q = |s: &str| css::parse_media(s).iter().map(|m| (m.min_w, m.max_w, m.never, m.neg)).collect::<Vec<_>>();
    if q("(400px <= width < 700px)") != [(400.0, 699.99, false, false)] || q("not print") != [(0.0, f32::INFINITY, true, true)] || q("(min-width: 40em)")[0].0 != 640.0 {
        return Err("@media: so'rov tahlili notöğri");
    }
    // :hover
    let a = d.elems.iter().position(|e| e.tag == "a").ok_or(":hover: <a> yöq")? as u16;
    let base = col(&d, "L");
    if !d.set_hover(a) || col(&d, "L") != Some(Some(0x0000ff)) {
        return Err(":hover: havola rangi özgarmadi");
    }
    let und = d.blocks.iter().flat_map(|b| b.spans.iter()).find(|s| s.text.trim() == "L").map(|s| s.underline);
    if und != Some(true) {
        return Err(":hover: text-decoration qöllanmadi");
    }
    d.set_hover(NO_EL);
    if col(&d, "L") != base {
        return Err(":hover: siçqonça ketganda uslub tiklanmadi");
    }
    let c = d.elems.iter().position(|e| e.classes.iter().any(|k| k == "c")).ok_or(":hover: .c yöq")? as u16;
    d.set_hover(c);
    if col(&d, "C") != Some(Some(0x123456)) {
        return Err(":hover: ajdod (.k:hover .c) qöllanmadi");
    }
    Ok(8)
}

/// CSS bezak: linear-gradient töxtaşlari / yönalişi va box-shadow parametrlari;
/// yaroqsiz qiymatlar panic qilmasdan e'tiborsiz qoldiriladi.
pub fn self_test_decor() -> Result<usize, &'static str> {
    let mut n = crate::gfx::self_test_decor()?;
    let g = |s: &str| css::parse_gradient(s);
    // yönalişlar
    for (src, ang) in [
        ("linear-gradient(red, blue)", 180u16),
        ("linear-gradient(to bottom, red, blue)", 180),
        ("linear-gradient(to right, red, blue)", 90),
        ("linear-gradient(to top, red, blue)", 0),
        ("linear-gradient(to left, red, blue)", 270),
        ("linear-gradient(to top right, red, blue)", 45),
        ("linear-gradient(0deg, red, blue)", 0),
        ("linear-gradient(90deg, red, blue)", 90),
        ("linear-gradient(270deg, red, blue)", 270),
        ("linear-gradient(-90deg, red, blue)", 270),
        ("linear-gradient(0.5turn, red, blue)", 180),
    ] {
        if g(src).map(|x| x.angle) != Some(ang) {
            return Err("linear-gradient: yönaliş notöğri");
        }
        n += 1;
    }
    // to'xtaşlar: 3 rang, joylar, rgb() içidagi vergullar
    let x = g("linear-gradient(45deg, #ff0000, rgb(0, 255, 0) 30%, #0000ff)").ok_or("linear-gradient: 3 töxtaş öqilmadi")?;
    if x.angle != 45 || x.n != 3 || x.stops[..3] != [(0xff0000, 0), (0x00ff00, 300), (0x0000ff, 1000)] {
        return Err("linear-gradient: töxtaşlar notöğri");
    }
    let x = g("linear-gradient(to right, #111 10%, #222 5%, #333)").ok_or("linear-gradient: joylar")?;
    if x.stops[..3] != [(0x111111, 100), (0x222222, 100), (0x333333, 1000)] {
        return Err("linear-gradient: kamayuvçi joy tuzatilmadi");
    }
    n += 2;
    // yaroqsizlar — None
    for bad in ["linear-gradient(", "linear-gradient(red, blue", "linear-gradient(red)", "linear-gradient(to middle, red, blue)", "linear-gradient(90deg, xyz, blue)", "linear-gradient()", "radial-gradient(red, blue)", "linear-gradient(((,,))"] {
        if g(bad).is_some() {
            return Err("linear-gradient: yaroqsiz qiymat qabul qilindi");
        }
        n += 1;
    }
    // box-shadow
    let s = css::parse_shadow("2px 4px 8px 1px rgba(0, 0, 0, .5)").ok_or("box-shadow: öqilmadi")?;
    if (s.dx, s.dy, s.blur, s.spread, s.color) != (2.0, 4.0, 8.0, 1.0, 0) || !(126..=129).contains(&s.alpha) {
        return Err("box-shadow: parametrlar notöğri");
    }
    let s = css::parse_shadow("#1e3a8a -3px 5px").ok_or("box-shadow: rang boşda")?;
    if (s.dx, s.dy, s.blur, s.spread, s.color, s.alpha) != (-3.0, 5.0, 0.0, 0.0, 0x1e3a8a, 255) {
        return Err("box-shadow: rang boşdagi variant notöğri");
    }
    for bad in ["none", "", "5px", "1px 2px 3px 4px 5px red", "foo bar", "inset 0 0 4px red", "1px 2px transparent"] {
        if css::parse_shadow(bad).is_some() {
            return Err("box-shadow: yaroqsiz qiymat qabul qilindi");
        }
        n += 1;
    }
    // Hujjat orqali: blok (konteyner) ga ötadi; yaroqsizlari fonni buzmaydi
    let d = parse("<style>.a{background:linear-gradient(to right,#ff0000,#0000ff);box-shadow:0 2px 6px #000;border-radius:8px}\
        .b{background:#00ff00;background:linear-gradient(#ff0000;box-shadow:bad}</style>\
        <div class=a><p>A</p></div><div class=b><p>B</p></div><div style=\"background-image:linear-gradient(0deg,#000,#fff)\"><p>C</p></div>");
    let blk = |t: &str| d.blocks.iter().find(|b| b.spans.iter().any(|s| s.text.trim() == t));
    let a = blk("A").ok_or("bezak: A bloki yöq")?;
    if a.grad.map(|g| (g.angle, g.n)) != Some((90, 2)) || a.bg != Some(0xff0000) || a.shadow.map(|s| (s.dy, s.blur)) != Some((2.0, 6.0)) || a.bgk == 0 {
        return Err("bezak: .a gradient/soya blokka ötmadi");
    }
    let b = blk("B").ok_or("bezak: B bloki yöq")?;
    if b.grad.is_some() || b.shadow.is_some() || b.bg != Some(0x00ff00) {
        return Err("bezak: yaroqsiz gradient fonni buzdi");
    }
    let c = blk("C").ok_or("bezak: C bloki yöq")?;
    if c.grad.map(|g| g.angle) != Some(0) {
        return Err("bezak: inline background-image öqilmadi");
    }
    Ok(n + 3)
}
