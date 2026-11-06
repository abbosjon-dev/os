//! JavaScript interpretatori — noldan (ES5/ES6 kiçik qismi).
//! Leksik tahlil → AST → daraxt böyiça bajariş. Qöllanadi: let/const/var,
//! funksiyalar (deklaratsiya, ifoda, strelkali), yopilmalar (closures), if/else,
//! for/while/do, for…of/in, break/continue, try/catch/throw, ?:, &&/||/??,
//! satr/son/massiv/obyekt, shablon satrlar `${…}`, Math, Date, JSON.stringify,
//! DOM: document.getElementById/querySelector/write/title, element.textContent/
//! innerHTML/style/onclick/addEventListener, console.log, alert, setTimeout/
//! setInterval. Cheksiz sikldan himoya — qadamlar çeklovi.

use crate::html::Doc;
use alloc::boxed::Box;
use alloc::format;
use alloc::rc::Rc;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::cell::RefCell;

// ---------------- Leksik tahlil ----------------

#[derive(Clone, PartialEq, Debug)]
enum Tok {
    Num(f64),
    Str(String),
    Tmpl(String),
    Id(String),
    P(&'static str),
    Eof,
}

const PUNCT: [&str; 48] = [
    "===", "!==", "**=", "...", "**", "=>", "==", "!=", "<=", ">=", "&&", "||", "??", "++", "--", "+=", "-=", "*=", "/=", "%=",
    "?.", "{", "}", "(", ")", "[", "]", ";", ",", ".", "<", ">", "+", "-", "*", "/", "%", "=", "!", "?", ":", "&", "|", "^",
    "~", "<<", ">>", "@",
];

fn lex(src: &str) -> Result<Vec<Tok>, String> {
    let b: Vec<char> = src.chars().collect();
    let mut i = 0;
    let mut out = Vec::new();
    while i < b.len() {
        let c = b[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c == '/' && i + 1 < b.len() && b[i + 1] == '/' {
            while i < b.len() && b[i] != '\n' {
                i += 1;
            }
            continue;
        }
        if c == '/' && i + 1 < b.len() && b[i + 1] == '*' {
            i += 2;
            while i + 1 < b.len() && !(b[i] == '*' && b[i + 1] == '/') {
                i += 1;
            }
            i += 2;
            continue;
        }
        if c.is_ascii_digit() || (c == '.' && i + 1 < b.len() && b[i + 1].is_ascii_digit()) {
            let st = i;
            if c == '0' && i + 1 < b.len() && (b[i + 1] == 'x' || b[i + 1] == 'X') {
                i += 2;
                let h0 = i;
                while i < b.len() && b[i].is_ascii_hexdigit() {
                    i += 1;
                }
                let h: String = b[h0..i].iter().collect();
                out.push(Tok::Num(u64::from_str_radix(&h, 16).unwrap_or(0) as f64));
                continue;
            }
            while i < b.len() && (b[i].is_ascii_digit() || b[i] == '.' || b[i] == '_') {
                i += 1;
            }
            let mut e = 0i32;
            let mut frac: String = b[st..i].iter().filter(|c| **c != '_').collect();
            if i < b.len() && (b[i] == 'e' || b[i] == 'E') {
                i += 1;
                let neg = i < b.len() && b[i] == '-';
                if i < b.len() && (b[i] == '-' || b[i] == '+') {
                    i += 1;
                }
                let e0 = i;
                while i < b.len() && b[i].is_ascii_digit() {
                    i += 1;
                }
                let es: String = b[e0..i].iter().collect();
                e = es.parse().unwrap_or(0);
                if neg {
                    e = -e;
                }
            }
            if frac.ends_with('.') {
                frac.pop();
            }
            let mut v = parse_num(&frac).unwrap_or(f64::NAN);
            if e != 0 {
                v *= powi(10.0, e);
            }
            out.push(Tok::Num(v));
            continue;
        }
        if c == '"' || c == '\'' || c == '`' {
            let q = c;
            i += 1;
            let mut s = String::new();
            while i < b.len() && b[i] != q {
                if b[i] == '\\' && i + 1 < b.len() {
                    i += 1;
                    if q == '`' {
                        // shablonda xom qoldiramiz (keyin yeçiladi)
                        s.push('\\');
                        s.push(b[i]);
                        i += 1;
                        continue;
                    }
                    s.push(match b[i] {
                        'n' => '\n',
                        't' => '\t',
                        'r' => '\r',
                        '0' => '\0',
                        'u' if i + 4 < b.len() => {
                            let h: String = b[i + 1..i + 5].iter().collect();
                            i += 4;
                            char::from_u32(u32::from_str_radix(&h, 16).unwrap_or(63)).unwrap_or('?')
                        }
                        x => x,
                    });
                    i += 1;
                    continue;
                }
                s.push(b[i]);
                i += 1;
            }
            i += 1;
            out.push(if q == '`' { Tok::Tmpl(s) } else { Tok::Str(s) });
            continue;
        }
        if c.is_alphabetic() || c == '_' || c == '$' {
            let st = i;
            while i < b.len() && (b[i].is_alphanumeric() || b[i] == '_' || b[i] == '$') {
                i += 1;
            }
            out.push(Tok::Id(b[st..i].iter().collect()));
            continue;
        }
        let rest: String = b[i..(i + 3).min(b.len())].iter().collect();
        match PUNCT.iter().find(|p| rest.starts_with(**p)) {
            Some(p) => {
                out.push(Tok::P(p));
                i += p.chars().count();
            }
            None => return Err(format!("Notanış belgi: {}", c)),
        }
    }
    out.push(Tok::Eof);
    Ok(out)
}

fn powi(b: f64, e: i32) -> f64 {
    let mut r = 1.0;
    for _ in 0..e.unsigned_abs() {
        r *= b;
    }
    if e < 0 { 1.0 / r } else { r }
}

fn parse_num(s: &str) -> Option<f64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let (neg, s) = match s.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, s.strip_prefix('+').unwrap_or(s)),
    };
    if s == "Infinity" {
        return Some(if neg { f64::NEG_INFINITY } else { f64::INFINITY });
    }
    let (ip, fp) = s.split_once('.').unwrap_or((s, ""));
    if !ip.chars().all(|c| c.is_ascii_digit()) || !fp.chars().all(|c| c.is_ascii_digit()) || (ip.is_empty() && fp.is_empty()) {
        return None;
    }
    let mut v = 0f64;
    for c in ip.bytes() {
        v = v * 10.0 + (c - b'0') as f64;
    }
    let mut sc = 0.1;
    for c in fp.bytes() {
        v += (c - b'0') as f64 * sc;
        sc *= 0.1;
    }
    Some(if neg { -v } else { v })
}

// ---------------- AST ----------------

#[derive(Clone)]
enum TPart {
    S(String),
    E(Expr),
}

#[derive(Clone)]
enum Expr {
    Num(f64),
    Str(Rc<str>),
    Tmpl(Vec<TPart>),
    Bool(bool),
    Null,
    Undef,
    Id(String),
    Arr(Vec<Expr>),
    Obj(Vec<(String, Expr)>),
    Fun(Rc<FDef>),
    Un(&'static str, Box<Expr>),
    Bin(&'static str, Box<Expr>, Box<Expr>),
    Assign(&'static str, Box<Expr>, Box<Expr>),
    Upd(bool, bool, Box<Expr>), // (prefiks, oşiriş)
    Call(Box<Expr>, Vec<Expr>),
    Mem(Box<Expr>, String),
    Idx(Box<Expr>, Box<Expr>),
    Cond(Box<Expr>, Box<Expr>, Box<Expr>),
    New(Box<Expr>, Vec<Expr>),
    Spread(Box<Expr>),
    Await(Box<Expr>),
}

#[derive(Clone)]
enum Stmt {
    Var(Vec<(String, Option<Expr>)>),
    Expr(Expr),
    If(Expr, Box<Stmt>, Option<Box<Stmt>>),
    While(Expr, Box<Stmt>),
    DoWhile(Box<Stmt>, Expr),
    For(Option<Box<Stmt>>, Option<Expr>, Option<Expr>, Box<Stmt>),
    ForOf(String, Expr, Box<Stmt>, bool), // true — of, false — in
    Block(Vec<Stmt>),
    Ret(Option<Expr>),
    Break,
    Cont,
    Fun(String, Rc<FDef>),
    Try(Vec<Stmt>, Option<(String, Vec<Stmt>)>, Vec<Stmt>),
    Throw(Expr),
    Empty,
}

pub struct FDef {
    params: Vec<String>,
    body: Vec<Stmt>,
    expr: Option<Expr>, // strelkali funksiya: qisqa tana
    is_async: core::cell::Cell<bool>,
}

// ---------------- Tahlilçi ----------------

struct Parser {
    t: Vec<Tok>,
    i: usize,
}

type PR<T> = Result<T, String>;

impl Parser {
    fn peek(&self) -> &Tok {
        &self.t[self.i.min(self.t.len() - 1)]
    }
    fn peek2(&self) -> &Tok {
        &self.t[(self.i + 1).min(self.t.len() - 1)]
    }
    fn next(&mut self) -> Tok {
        let t = self.peek().clone();
        self.i += 1;
        t
    }
    fn is(&self, p: &str) -> bool {
        matches!(self.peek(), Tok::P(x) if *x == p)
    }
    fn is_id(&self, k: &str) -> bool {
        matches!(self.peek(), Tok::Id(x) if x == k)
    }
    fn eat(&mut self, p: &str) -> bool {
        if self.is(p) {
            self.i += 1;
            true
        } else {
            false
        }
    }
    fn expect(&mut self, p: &str) -> PR<()> {
        if self.eat(p) { Ok(()) } else { Err(format!("'{}' kutilgan edi, {:?} keldi", p, self.peek())) }
    }
    fn ident(&mut self) -> PR<String> {
        match self.next() {
            Tok::Id(s) => Ok(s),
            t => Err(format!("nom kutilgan edi, {:?} keldi", t)),
        }
    }
    fn semi(&mut self) {
        self.eat(";");
    }

    fn program(&mut self) -> PR<Vec<Stmt>> {
        let mut v = Vec::new();
        while *self.peek() != Tok::Eof {
            v.push(self.stmt()?);
        }
        Ok(v)
    }
    fn block(&mut self) -> PR<Vec<Stmt>> {
        self.expect("{")?;
        let mut v = Vec::new();
        while !self.is("}") {
            if *self.peek() == Tok::Eof {
                return Err(String::from("'}' yetişmaydi"));
            }
            v.push(self.stmt()?);
        }
        self.i += 1;
        Ok(v)
    }
    fn var_decl(&mut self) -> PR<Stmt> {
        self.i += 1; // let/const/var
        let mut v = Vec::new();
        loop {
            let n = self.ident()?;
            let init = if self.eat("=") { Some(self.assign()?) } else { None };
            v.push((n, init));
            if !self.eat(",") {
                break;
            }
        }
        Ok(Stmt::Var(v))
    }
    fn stmt(&mut self) -> PR<Stmt> {
        if self.is("{") {
            return Ok(Stmt::Block(self.block()?));
        }
        if self.eat(";") {
            return Ok(Stmt::Empty);
        }
        if let Tok::Id(k) = self.peek().clone() {
            match k.as_str() {
                "let" | "const" | "var" => {
                    let s = self.var_decl()?;
                    self.semi();
                    return Ok(s);
                }
                "function" if matches!(self.peek2(), Tok::Id(_)) => {
                    self.i += 1;
                    let n = self.ident()?;
                    let f = self.fn_rest()?;
                    return Ok(Stmt::Fun(n, f));
                }
                "async" if matches!(self.peek2(), Tok::Id(k) if k == "function") => {
                    self.i += 2;
                    let n = self.ident()?;
                    let f = self.fn_rest()?;
                    f.is_async.set(true);
                    return Ok(Stmt::Fun(n, f));
                }
                "if" => {
                    self.i += 1;
                    self.expect("(")?;
                    let c = self.expr()?;
                    self.expect(")")?;
                    let a = self.stmt()?;
                    let b = if self.is_id("else") {
                        self.i += 1;
                        Some(Box::new(self.stmt()?))
                    } else {
                        None
                    };
                    return Ok(Stmt::If(c, Box::new(a), b));
                }
                "while" => {
                    self.i += 1;
                    self.expect("(")?;
                    let c = self.expr()?;
                    self.expect(")")?;
                    return Ok(Stmt::While(c, Box::new(self.stmt()?)));
                }
                "do" => {
                    self.i += 1;
                    let body = self.stmt()?;
                    if !self.is_id("while") {
                        return Err(String::from("do…while"));
                    }
                    self.i += 1;
                    self.expect("(")?;
                    let c = self.expr()?;
                    self.expect(")")?;
                    self.semi();
                    return Ok(Stmt::DoWhile(Box::new(body), c));
                }
                "for" => {
                    self.i += 1;
                    self.expect("(")?;
                    // for (let x of / in …)
                    let save = self.i;
                    if self.is_id("let") || self.is_id("const") || self.is_id("var") {
                        self.i += 1;
                    }
                    if let Tok::Id(n) = self.peek().clone() {
                        if matches!(self.peek2(), Tok::Id(k) if k == "of" || k == "in") {
                            let of = matches!(self.peek2(), Tok::Id(k) if k == "of");
                            self.i += 2;
                            let it = self.expr()?;
                            self.expect(")")?;
                            return Ok(Stmt::ForOf(n, it, Box::new(self.stmt()?), of));
                        }
                    }
                    self.i = save;
                    let init = if self.is(";") {
                        None
                    } else if self.is_id("let") || self.is_id("const") || self.is_id("var") {
                        Some(Box::new(self.var_decl()?))
                    } else {
                        Some(Box::new(Stmt::Expr(self.expr()?)))
                    };
                    self.expect(";")?;
                    let cond = if self.is(";") { None } else { Some(self.expr()?) };
                    self.expect(";")?;
                    let upd = if self.is(")") { None } else { Some(self.expr()?) };
                    self.expect(")")?;
                    return Ok(Stmt::For(init, cond, upd, Box::new(self.stmt()?)));
                }
                "return" => {
                    self.i += 1;
                    let e = if self.is(";") || self.is("}") || *self.peek() == Tok::Eof { None } else { Some(self.expr()?) };
                    self.semi();
                    return Ok(Stmt::Ret(e));
                }
                "break" => {
                    self.i += 1;
                    self.semi();
                    return Ok(Stmt::Break);
                }
                "continue" => {
                    self.i += 1;
                    self.semi();
                    return Ok(Stmt::Cont);
                }
                "throw" => {
                    self.i += 1;
                    let e = self.expr()?;
                    self.semi();
                    return Ok(Stmt::Throw(e));
                }
                "try" => {
                    self.i += 1;
                    let body = self.block()?;
                    let mut catch = None;
                    if self.is_id("catch") {
                        self.i += 1;
                        let n = if self.eat("(") {
                            let n = self.ident()?;
                            self.expect(")")?;
                            n
                        } else {
                            String::from("_")
                        };
                        catch = Some((n, self.block()?));
                    }
                    let fin = if self.is_id("finally") {
                        self.i += 1;
                        self.block()?
                    } else {
                        Vec::new()
                    };
                    return Ok(Stmt::Try(body, catch, fin));
                }
                _ => {}
            }
        }
        let e = self.expr()?;
        self.semi();
        Ok(Stmt::Expr(e))
    }
    fn fn_rest(&mut self) -> PR<Rc<FDef>> {
        self.expect("(")?;
        let mut params = Vec::new();
        while !self.is(")") {
            self.eat("...");
            params.push(self.ident()?);
            if self.eat("=") {
                self.assign()?; // standart qiymat — taşlanadi
            }
            if !self.eat(",") {
                break;
            }
        }
        self.expect(")")?;
        let body = self.block()?;
        Ok(Rc::new(FDef { params, body, expr: None, is_async: core::cell::Cell::new(false) }))
    }
    fn expr(&mut self) -> PR<Expr> {
        let e = self.assign()?;
        if self.is(",") && !matches!(self.peek2(), Tok::Eof) {
            // vergul operatori: oxirgisi qaytadi
            let mut last = e;
            while self.eat(",") {
                last = self.assign()?;
            }
            return Ok(last);
        }
        Ok(e)
    }
    /// Strelkali funksiya boşlanayaptimi?  x => …  yoki  (a, b) => …
    fn arrow_ahead(&self) -> bool {
        if matches!(self.peek(), Tok::Id(_)) && matches!(self.peek2(), Tok::P("=>")) {
            return true;
        }
        if !self.is("(") {
            return false;
        }
        let mut d = 0;
        let mut k = self.i;
        while k < self.t.len() {
            match &self.t[k] {
                Tok::P("(") => d += 1,
                Tok::P(")") => {
                    d -= 1;
                    if d == 0 {
                        return matches!(self.t.get(k + 1), Some(Tok::P("=>")));
                    }
                }
                Tok::Eof => return false,
                _ => {}
            }
            k += 1;
        }
        false
    }
    fn arrow(&mut self) -> PR<Expr> {
        let mut params = Vec::new();
        if let Tok::Id(n) = self.peek().clone() {
            self.i += 1;
            params.push(n);
        } else {
            self.expect("(")?;
            while !self.is(")") {
                self.eat("...");
                params.push(self.ident()?);
                if self.eat("=") {
                    self.assign()?;
                }
                if !self.eat(",") {
                    break;
                }
            }
            self.expect(")")?;
        }
        self.expect("=>")?;
        if self.is("{") {
            let body = self.block()?;
            Ok(Expr::Fun(Rc::new(FDef { params, body, expr: None, is_async: core::cell::Cell::new(false) })))
        } else {
            let e = self.assign()?;
            Ok(Expr::Fun(Rc::new(FDef { params, body: Vec::new(), expr: Some(e), is_async: core::cell::Cell::new(false) })))
        }
    }
    fn assign(&mut self) -> PR<Expr> {
        if self.is_id("async") && !matches!(self.peek2(), Tok::Id(k) if k == "function") {
            let save = self.i;
            self.i += 1;
            if self.arrow_ahead() {
                let f = self.arrow()?;
                if let Expr::Fun(d) = &f {
                    d.is_async.set(true);
                }
                return Ok(f);
            }
            self.i = save;
        }
        if self.arrow_ahead() {
            return self.arrow();
        }
        let lhs = self.cond()?;
        for op in ["=", "+=", "-=", "*=", "/=", "%=", "**="] {
            if self.is(op) {
                self.i += 1;
                let rhs = self.assign()?;
                let op: &'static str = PUNCT.iter().find(|p| **p == op).unwrap();
                return Ok(Expr::Assign(op, Box::new(lhs), Box::new(rhs)));
            }
        }
        Ok(lhs)
    }
    fn cond(&mut self) -> PR<Expr> {
        let c = self.bin(0)?;
        if self.eat("?") {
            let a = self.assign()?;
            self.expect(":")?;
            let b = self.assign()?;
            return Ok(Expr::Cond(Box::new(c), Box::new(a), Box::new(b)));
        }
        Ok(c)
    }
    fn bin(&mut self, lvl: usize) -> PR<Expr> {
        const LV: [&[&str]; 10] = [
            &["??"],
            &["||"],
            &["&&"],
            &["|"],
            &["^"],
            &["&"],
            &["===", "!==", "==", "!="],
            &["<", ">", "<=", ">=", "instanceof", "in"],
            &["+", "-"],
            &["*", "/", "%"],
        ];
        if lvl >= LV.len() {
            return self.pow();
        }
        let mut l = self.bin(lvl + 1)?;
        loop {
            let op = match self.peek() {
                Tok::P(p) if LV[lvl].contains(p) => *p,
                Tok::Id(k) if (k == "instanceof" || k == "in") && LV[lvl].contains(&k.as_str()) => {
                    if k == "in" { "in" } else { "instanceof" }
                }
                _ => break,
            };
            self.i += 1;
            let r = self.bin(lvl + 1)?;
            l = Expr::Bin(op, Box::new(l), Box::new(r));
        }
        Ok(l)
    }
    fn pow(&mut self) -> PR<Expr> {
        let b = self.unary()?;
        if self.eat("**") {
            let e = self.pow()?;
            return Ok(Expr::Bin("**", Box::new(b), Box::new(e)));
        }
        Ok(b)
    }
    fn unary(&mut self) -> PR<Expr> {
        for op in ["!", "-", "+", "~"] {
            if self.is(op) {
                self.i += 1;
                let e = self.unary()?;
                let op: &'static str = PUNCT.iter().find(|p| **p == op).unwrap();
                return Ok(Expr::Un(op, Box::new(e)));
            }
        }
        if self.is("++") || self.is("--") {
            let inc = self.is("++");
            self.i += 1;
            let e = self.unary()?;
            return Ok(Expr::Upd(true, inc, Box::new(e)));
        }
        if let Tok::Id(k) = self.peek().clone() {
            match k.as_str() {
                "typeof" => {
                    self.i += 1;
                    return Ok(Expr::Un("typeof", Box::new(self.unary()?)));
                }
                "await" => {
                    self.i += 1;
                    let e = self.unary()?;
                    return Ok(Expr::Await(Box::new(e)));
                }
                "void" | "delete" => {
                    self.i += 1;
                    let e = self.unary()?;
                    return Ok(if k == "void" { Expr::Un("void", Box::new(e)) } else { e });
                }
                _ => {}
            }
        }
        let e = self.postfix()?;
        if self.is("++") || self.is("--") {
            let inc = self.is("++");
            self.i += 1;
            return Ok(Expr::Upd(false, inc, Box::new(e)));
        }
        Ok(e)
    }
    fn args(&mut self) -> PR<Vec<Expr>> {
        let mut a = Vec::new();
        while !self.is(")") {
            if self.eat("...") {
                a.push(Expr::Spread(Box::new(self.assign()?)));
            } else {
                a.push(self.assign()?);
            }
            if !self.eat(",") {
                break;
            }
        }
        self.expect(")")?;
        Ok(a)
    }
    fn postfix(&mut self) -> PR<Expr> {
        let mut e = if self.is_id("new") {
            self.i += 1;
            let mut c = self.primary()?;
            while self.eat(".") {
                c = Expr::Mem(Box::new(c), self.ident()?);
            }
            let a = if self.eat("(") { self.args()? } else { Vec::new() };
            Expr::New(Box::new(c), a)
        } else {
            self.primary()?
        };
        loop {
            if self.eat(".") || self.eat("?.") {
                e = Expr::Mem(Box::new(e), self.ident()?);
            } else if self.eat("[") {
                let i = self.expr()?;
                self.expect("]")?;
                e = Expr::Idx(Box::new(e), Box::new(i));
            } else if self.eat("(") {
                e = Expr::Call(Box::new(e), self.args()?);
            } else if let Tok::Tmpl(_) = self.peek() {
                // teglangan shablon — oddiy chaqiriş sifatida
                let t = self.primary()?;
                e = Expr::Call(Box::new(e), alloc::vec![t]);
            } else {
                break;
            }
        }
        Ok(e)
    }
    fn primary(&mut self) -> PR<Expr> {
        match self.next() {
            Tok::Num(n) => Ok(Expr::Num(n)),
            Tok::Str(s) => Ok(Expr::Str(Rc::from(s.as_str()))),
            Tok::Tmpl(raw) => Ok(Expr::Tmpl(parse_tmpl(&raw)?)),
            Tok::P("(") => {
                let e = self.expr()?;
                self.expect(")")?;
                Ok(e)
            }
            Tok::P("[") => {
                let mut v = Vec::new();
                while !self.is("]") {
                    if self.eat("...") {
                        v.push(Expr::Spread(Box::new(self.assign()?)));
                    } else {
                        v.push(self.assign()?);
                    }
                    if !self.eat(",") {
                        break;
                    }
                }
                self.expect("]")?;
                Ok(Expr::Arr(v))
            }
            Tok::P("{") => {
                let mut v = Vec::new();
                while !self.is("}") {
                    let key = match self.next() {
                        Tok::Id(s) | Tok::Str(s) => s,
                        Tok::Num(n) => num_str(n),
                        t => return Err(format!("obyekt kaliti: {:?}", t)),
                    };
                    if self.is("(") {
                        let f = self.fn_rest()?;
                        v.push((key, Expr::Fun(f)));
                    } else if self.eat(":") {
                        v.push((key, self.assign()?));
                    } else {
                        v.push((key.clone(), Expr::Id(key)));
                    }
                    if !self.eat(",") {
                        break;
                    }
                }
                self.expect("}")?;
                Ok(Expr::Obj(v))
            }
            Tok::Id(k) => Ok(match k.as_str() {
                "true" => Expr::Bool(true),
                "false" => Expr::Bool(false),
                "null" => Expr::Null,
                "undefined" => Expr::Undef,
                "NaN" => Expr::Num(f64::NAN),
                "Infinity" => Expr::Num(f64::INFINITY),
                "function" => {
                    if matches!(self.peek(), Tok::Id(_)) {
                        self.i += 1;
                    }
                    Expr::Fun(self.fn_rest()?)
                }
                "async" if self.is_id("function") => {
                    self.i += 1;
                    if matches!(self.peek(), Tok::Id(_)) {
                        self.i += 1;
                    }
                    let f = self.fn_rest()?;
                    f.is_async.set(true);
                    Expr::Fun(f)
                }
                _ => Expr::Id(k),
            }),
            t => Err(format!("kutilmagan: {:?}", t)),
        }
    }
}

fn parse_tmpl(raw: &str) -> PR<Vec<TPart>> {
    let mut parts = Vec::new();
    let mut s = String::new();
    let c: Vec<char> = raw.chars().collect();
    let mut i = 0;
    while i < c.len() {
        if c[i] == '\\' && i + 1 < c.len() {
            s.push(match c[i + 1] {
                'n' => '\n',
                't' => '\t',
                x => x,
            });
            i += 2;
            continue;
        }
        if c[i] == '$' && i + 1 < c.len() && c[i + 1] == '{' {
            let mut d = 1;
            let mut j = i + 2;
            while j < c.len() && d > 0 {
                if c[j] == '{' {
                    d += 1;
                } else if c[j] == '}' {
                    d -= 1;
                }
                j += 1;
            }
            let inner: String = c[i + 2..j - 1].iter().collect();
            if !s.is_empty() {
                parts.push(TPart::S(core::mem::take(&mut s)));
            }
            let mut p = Parser { t: lex(&inner)?, i: 0 };
            parts.push(TPart::E(p.expr()?));
            i = j;
            continue;
        }
        s.push(c[i]);
        i += 1;
    }
    if !s.is_empty() {
        parts.push(TPart::S(s));
    }
    Ok(parts)
}

// ---------------- Qiymatlar ----------------

#[derive(Clone)]
pub enum Value {
    Undef,
    Null,
    Bool(bool),
    Num(f64),
    Str(Rc<str>),
    Arr(Rc<RefCell<Vec<Value>>>),
    Obj(Rc<RefCell<Vec<(String, Value)>>>),
    Func(Rc<FDef>, Env),
    Native(&'static str, Box<Value>), // (nomi, this)
    Host(&'static str),               // document, console, Math, …
    Elem(usize),
    Style(usize),
    Date(f64),
    Promise(Rc<RefCell<Prom>>),
}

/// Promise holati va kutayotgan davomlar (then).
pub struct Prom {
    state: PState,
    cbs: Vec<(Value, Value, Rc<RefCell<Prom>>)>,
}

#[derive(Clone)]
enum PState {
    Pending,
    Ok(Value),
    Err(Value),
}

fn new_prom() -> Rc<RefCell<Prom>> {
    Rc::new(RefCell::new(Prom { state: PState::Pending, cbs: Vec::new() }))
}

type Env = Rc<RefCell<Scope>>;

pub struct Scope {
    vars: Vec<(String, Value)>,
    parent: Option<Env>,
}

fn new_env(parent: Option<Env>) -> Env {
    Rc::new(RefCell::new(Scope { vars: Vec::new(), parent }))
}

fn num_str(n: f64) -> String {
    if n.is_nan() {
        return String::from("NaN");
    }
    if n.is_infinite() {
        return String::from(if n > 0.0 { "Infinity" } else { "-Infinity" });
    }
    if n == (n as i64) as f64 && n.abs() < 1e21 {
        return format!("{}", n as i64);
    }
    format!("{}", n)
}

impl Value {
    fn str(s: &str) -> Value {
        Value::Str(Rc::from(s))
    }
    fn truthy(&self) -> bool {
        match self {
            Value::Undef | Value::Null => false,
            Value::Bool(b) => *b,
            Value::Num(n) => *n != 0.0 && !n.is_nan(),
            Value::Str(s) => !s.is_empty(),
            _ => true,
        }
    }
    fn num(&self) -> f64 {
        match self {
            Value::Num(n) => *n,
            Value::Bool(b) => *b as u8 as f64,
            Value::Null => 0.0,
            Value::Str(s) => {
                let t = s.trim();
                if t.is_empty() { 0.0 } else { parse_num(t).unwrap_or(f64::NAN) }
            }
            Value::Date(d) => *d,
            Value::Arr(a) if a.borrow().len() == 1 => a.borrow()[0].num(),
            Value::Arr(a) if a.borrow().is_empty() => 0.0,
            _ => f64::NAN,
        }
    }
    pub fn to_string(&self) -> String {
        match self {
            Value::Undef => String::from("undefined"),
            Value::Null => String::from("null"),
            Value::Bool(b) => String::from(if *b { "true" } else { "false" }),
            Value::Num(n) => num_str(*n),
            Value::Str(s) => String::from(&**s),
            Value::Arr(a) => a.borrow().iter().map(|v| if matches!(v, Value::Undef | Value::Null) { String::new() } else { v.to_string() }).collect::<Vec<_>>().join(","),
            Value::Obj(_) => String::from("[object Object]"),
            Value::Func(..) | Value::Native(..) => String::from("function () { [native code] }"),
            Value::Host(h) => format!("[object {}]", h),
            Value::Elem(_) => String::from("[object HTMLElement]"),
            Value::Style(_) => String::from("[object CSSStyleDeclaration]"),
            Value::Date(d) => date_str(*d, 0),
            Value::Promise(_) => String::from("[object Promise]"),
        }
    }
    fn type_of(&self) -> &'static str {
        match self {
            Value::Undef => "undefined",
            Value::Null => "object",
            Value::Bool(_) => "boolean",
            Value::Num(_) => "number",
            Value::Str(_) => "string",
            Value::Func(..) | Value::Native(..) => "function",
            _ => "object",
        }
    }
}

fn json(v: &Value, depth: usize) -> String {
    if depth > 8 {
        return String::from("null");
    }
    match v {
        Value::Str(s) => format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n")),
        Value::Num(n) if n.is_finite() => num_str(*n),
        Value::Num(_) | Value::Null | Value::Undef => String::from("null"),
        Value::Bool(_) => v.to_string(),
        Value::Arr(a) => format!("[{}]", a.borrow().iter().map(|x| json(x, depth + 1)).collect::<Vec<_>>().join(",")),
        Value::Obj(o) => format!(
            "{{{}}}",
            o.borrow().iter().filter(|(_, x)| !matches!(x, Value::Func(..) | Value::Undef)).map(|(k, x)| format!("\"{}\":{}", k, json(x, depth + 1))).collect::<Vec<_>>().join(",")
        ),
        _ => String::from("null"),
    }
}

/// Sana: 0 — toString, 1 — sana, 2 — vaqt.
fn date_str(ms: f64, kind: u8) -> String {
    let t = crate::time::local((ms / 1000.0) as u64);
    match kind {
        1 => format!("{:02}.{:02}.{}", t.day, t.month, t.year),
        2 => format!("{:02}:{:02}:{:02}", t.hour, t.minute, t.second),
        _ => format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", t.year, t.month, t.day, t.hour, t.minute, t.second),
    }
}

// ---------------- Bajariş ----------------

enum Ctl {
    Ret(Value),
    Brk,
    Cont,
    Throw(Value),
}

type R = Result<Value, Ctl>;

fn err(msg: &str) -> Ctl {
    Ctl::Throw(Value::str(msg))
}

pub struct Timer {
    at: u64,
    every: Option<u64>,
    f: Value,
    id: u32,
}

/// Sahifaning JS muhiti (global ozgaruvçilar, taymerlar, hodisa işlovçilari).
pub struct Engine {
    globals: Env,
    timers: Vec<Timer>,
    handlers: Vec<(usize, Value)>,
    next_id: u32,
    pub log: Vec<String>,
    pub alerts: Vec<String>,
    steps: u32,
    now: u64,
    pub errors: usize,
    /// Mikrovazifalar: (muvaffaqiyat işlovçisi, xato işlovçisi, keyingi promise, qiymat, muvaffaqiyatmi)
    micro: Vec<(Value, Value, Rc<RefCell<Prom>>, Value, bool)>,
    /// fetch() sörovlari: (raqam, usul, manzil, tana) — GUI ularni fonda yuklaydi
    pub fetch_out: Vec<(u32, String, String, Option<String>)>,
    fetch_wait: Vec<(u32, Rc<RefCell<Prom>>)>,
    submit_handlers: Vec<(usize, Value)>,
    prevented: bool,
    /// Joriy sahifa manzili (location uçun)
    pub url: String,
    /// await uçun: fetch ni joyida (sinxron) bajariş — GUI beradi
    pub fetch_hook: Option<fn(&str, &str, Option<&str>) -> Result<(u16, String, String), String>>,
    /// <canvas> 2D kontekstlari: (canvas raqami, kontekst obyekti)
    ctxs: Vec<(usize, Value)>,
}

const MAX_STEPS: u32 = 3_000_000;

impl Engine {
    pub fn new() -> Engine {
        let g = new_env(None);
        for h in ["document", "console", "Math", "window", "JSON", "Object", "Array", "Date", "Number", "String", "navigator", "location", "localStorage", "sessionStorage"] {
            g.borrow_mut().vars.push((String::from(h), Value::Host(h)));
        }
        g.borrow_mut().vars.push((String::from("Promise"), Value::Host("Promise")));
        g.borrow_mut().vars.push((String::from("URLSearchParams"), Value::Host("URLSearchParams")));
        for f in ["fetch", "encodeURIComponent", "decodeURIComponent", "encodeURI", "decodeURI", "__sl", "alert", "setTimeout", "setInterval", "clearTimeout", "clearInterval", "parseInt", "parseFloat", "isNaN", "isFinite", "prompt", "confirm"] {
            g.borrow_mut().vars.push((String::from(f), Value::Native(f, Box::new(Value::Undef))));
        }
        Engine {
            globals: g,
            timers: Vec::new(),
            handlers: Vec::new(),
            next_id: 1,
            log: Vec::new(),
            alerts: Vec::new(),
            steps: 0,
            now: 0,
            errors: 0,
            micro: Vec::new(),
            fetch_out: Vec::new(),
            fetch_wait: Vec::new(),
            submit_handlers: Vec::new(),
            prevented: false,
            url: String::new(),
            fetch_hook: None,
            ctxs: Vec::new(),
        }
    }

    /// Kodni bajaradi. Xato bölsa — matni (konsolga ham yoziladi).
    pub fn run(&mut self, code: &str, doc: &mut Doc, now: u64) -> Result<(), String> {
        self.steps = 0;
        self.now = now;
        let prog = Parser { t: lex(code)?, i: 0 }.program().inspect_err(|e| {
            self.errors += 1;
            self.log.push(format!("Sintaksis xatosi: {}", e));
        })?;
        let g = self.globals.clone();
        let r = self.exec_block(&prog, &g, doc);
        self.drain(doc);
        match r {
            Ok(_) | Err(Ctl::Ret(_)) => Ok(()),
            Err(Ctl::Throw(v)) => {
                self.errors += 1;
                let m = format!("Xato: {}", v.to_string());
                self.log.push(m.clone());
                Err(m)
            }
            Err(_) => Ok(()),
        }
    }

    /// Vaqti kelgan taymerlarni bajaradi. Bajarilgan bölsa — true.
    pub fn tick(&mut self, doc: &mut Doc, now: u64) -> bool {
        let mut ran = false;
        let mut k = 0;
        while k < self.timers.len() {
            if self.timers[k].at <= now {
                let f = self.timers[k].f.clone();
                match self.timers[k].every {
                    Some(iv) => {
                        self.timers[k].at = now + iv.max(16);
                        k += 1;
                    }
                    None => {
                        self.timers.remove(k);
                    }
                }
                self.steps = 0;
                self.now = now;
                self.call_top(&f, Vec::new(), doc);
                ran = true;
            } else {
                k += 1;
            }
        }
        if ran {
            self.drain(doc);
        }
        ran
    }

    /// await kutayotganda bir qadam: kutilayotgan fetch lar joyida bajariladi, aks holda
    /// eng yaqin taymergaça uxlab, uni işga tuşiradi. Hech narsa bölmasa — false.
    fn pump_sync(&mut self, doc: &mut Doc) -> bool {
        if !self.fetch_out.is_empty() {
            if let Some(hook) = self.fetch_hook {
                for (id, method, url, body) in core::mem::take(&mut self.fetch_out) {
                    let full = crate::net::http::join(&self.url, &url);
                    let r = hook(&method, &full, body.as_deref());
                    self.fetch_done(id, r, doc);
                }
                return true;
            }
        }
        if let Some(at) = self.timers.iter().map(|t| t.at).min() {
            let now = crate::timer::uptime_ms();
            if at > now {
                crate::task::sleep_ms((at - now).min(50));
            }
            let now = crate::timer::uptime_ms();
            self.tick(doc, now);
            return true;
        }
        false
    }

    /// Promise davomlarini (then) bajaradi.
    fn drain(&mut self, doc: &mut Doc) {
        let mut n = 0;
        while !self.micro.is_empty() && n < 10_000 {
            n += 1;
            let (fo, fe, next, v, ok) = self.micro.remove(0);
            let h = if ok { fo } else { fe };
            if matches!(h, Value::Func(..) | Value::Native(..)) {
                self.steps = 0;
                match self.call(&h, alloc::vec![v], Value::Undef, doc) {
                    Ok(r) => self.resolve(&next, r),
                    Err(Ctl::Throw(e)) => self.settle(&next, false, e),
                    Err(_) => self.settle(&next, true, Value::Undef),
                }
            } else {
                self.settle(&next, ok, v);
            }
        }
    }

    /// Promise ni qiymat bilan yeçadi (qiymat özi Promise bölsa — unga ulanadi).
    fn resolve(&mut self, p: &Rc<RefCell<Prom>>, v: Value) {
        if let Value::Promise(q) = &v {
            if Rc::ptr_eq(p, q) {
                return;
            }
            let st = q.borrow().state.clone();
            match st {
                PState::Ok(x) => self.settle(p, true, x),
                PState::Err(x) => self.settle(p, false, x),
                PState::Pending => q.borrow_mut().cbs.push((Value::Undef, Value::Undef, p.clone())),
            }
            return;
        }
        self.settle(p, true, v);
    }

    fn settle(&mut self, p: &Rc<RefCell<Prom>>, ok: bool, v: Value) {
        let cbs = {
            let mut b = p.borrow_mut();
            if !matches!(b.state, PState::Pending) {
                return;
            }
            b.state = if ok { PState::Ok(v.clone()) } else { PState::Err(v.clone()) };
            core::mem::take(&mut b.cbs)
        };
        for (fo, fe, next) in cbs {
            self.micro.push((fo, fe, next, v.clone(), ok));
        }
    }

    fn then(&mut self, p: &Rc<RefCell<Prom>>, fo: Value, fe: Value) -> Value {
        let next = new_prom();
        let st = p.borrow().state.clone();
        match st {
            PState::Pending => p.borrow_mut().cbs.push((fo, fe, next.clone())),
            PState::Ok(v) => self.micro.push((fo, fe, next.clone(), v, true)),
            PState::Err(v) => self.micro.push((fo, fe, next.clone(), v, false)),
        }
        Value::Promise(next)
    }

    /// fetch() javobi keldi (GUI çaqiradi): javob obyekti bilan promise yeçiladi.
    pub fn fetch_done(&mut self, id: u32, res: Result<(u16, String, String), String>, doc: &mut Doc) {
        let Some(k) = self.fetch_wait.iter().position(|(i, _)| *i == id) else { return };
        let (_, p) = self.fetch_wait.remove(k);
        match res {
            Ok((status, body, url)) => {
                let b = Value::str(&body);
                let o = alloc::vec![
                    (String::from("status"), Value::Num(status as f64)),
                    (String::from("ok"), Value::Bool((200..300).contains(&status))),
                    (String::from("url"), Value::str(&url)),
                    (String::from("text"), Value::Native("resp.text", Box::new(b.clone()))),
                    (String::from("json"), Value::Native("resp.json", Box::new(b))),
                ];
                self.settle(&p, true, Value::Obj(Rc::new(RefCell::new(o))));
            }
            Err(e) => self.settle(&p, false, Value::str(&format!("TypeError: fetch: {}", e))),
        }
        self.drain(doc);
    }

    /// Forma yuborilyapti: submit işlovçilari. true — preventDefault() çaqirildi.
    pub fn submit(&mut self, el: usize, doc: &mut Doc, now: u64) -> bool {
        self.prevented = false;
        let hs: Vec<Value> = self.submit_handlers.iter().filter(|(e, _)| *e == el).map(|(_, f)| f.clone()).collect();
        let ev = Value::Obj(Rc::new(RefCell::new(alloc::vec![
            (String::from("preventDefault"), Value::Native("ev.prevent", Box::new(Value::Undef))),
            (String::from("target"), Value::Elem(el)),
        ])));
        for f in hs {
            self.steps = 0;
            self.now = now;
            self.call_top(&f, alloc::vec![ev.clone()], doc);
        }
        self.drain(doc);
        self.prevented
    }

    /// Element bosildi: onclick atributi va addEventListener işlovçilari.
    pub fn click(&mut self, el: usize, doc: &mut Doc, now: u64) -> bool {
        // Hodisa qalqişi: element va uning ajdodlari işlovçilari
        let mut chain = Vec::new();
        let mut cur = el as u16;
        while cur != crate::html::NO_EL && chain.len() < 64 {
            chain.push(cur as usize);
            cur = doc.elems.get(cur as usize).map(|e| e.parent).unwrap_or(crate::html::NO_EL);
        }
        let mut ran = false;
        for el in chain {
            ran |= self.click_one(el, doc, now);
        }
        ran
    }

    fn click_one(&mut self, el: usize, doc: &mut Doc, now: u64) -> bool {
        let mut ran = false;
        if let Some(code) = doc.elems.get(el).and_then(|e| e.onclick.clone()) {
            let _ = self.run(&code, doc, now);
            ran = true;
        }
        let hs: Vec<Value> = self.handlers.iter().filter(|(e, _)| *e == el).map(|(_, f)| f.clone()).collect();
        for f in hs {
            self.steps = 0;
            self.now = now;
            self.call_top(&f, alloc::vec![Value::Undef], doc);
            ran = true;
        }
        if ran {
            self.drain(doc);
        }
        ran
    }

    pub fn has_handler(&self, el: usize, doc: &Doc) -> bool {
        let mut cur = el as u16;
        let mut n = 0;
        while cur != crate::html::NO_EL && n < 64 {
            let c = cur as usize;
            if self.handlers.iter().any(|(e, _)| *e == c) || doc.elems.get(c).is_some_and(|e| e.onclick.is_some()) {
                return true;
            }
            cur = doc.elems.get(c).map(|e| e.parent).unwrap_or(crate::html::NO_EL);
            n += 1;
        }
        false
    }

    fn call_top(&mut self, f: &Value, args: Vec<Value>, doc: &mut Doc) {
        if let Err(Ctl::Throw(v)) = self.call(f, args, Value::Undef, doc) {
            self.errors += 1;
            self.log.push(format!("Xato: {}", v.to_string()));
        }
    }

    fn step(&mut self) -> Result<(), Ctl> {
        self.steps += 1;
        if self.steps > MAX_STEPS {
            return Err(err("Skript juda uzoq işladi (to'xtatildi)"));
        }
        Ok(())
    }

    fn lookup(&self, env: &Env, name: &str) -> Option<Value> {
        let mut cur = Some(env.clone());
        while let Some(e) = cur {
            let b = e.borrow();
            if let Some((_, v)) = b.vars.iter().rev().find(|(n, _)| n == name) {
                return Some(v.clone());
            }
            cur = b.parent.clone();
        }
        None
    }

    fn set_var(&self, env: &Env, name: &str, v: Value) {
        let mut cur = Some(env.clone());
        while let Some(e) = cur {
            let mut b = e.borrow_mut();
            if let Some(slot) = b.vars.iter_mut().rev().find(|(n, _)| n == name) {
                slot.1 = v;
                return;
            }
            cur = b.parent.clone();
        }
        self.globals.borrow_mut().vars.push((String::from(name), v));
    }

    fn exec_block(&mut self, stmts: &[Stmt], env: &Env, doc: &mut Doc) -> R {
        // Funksiya deklaratsiyalari oldindan (hoisting)
        for s in stmts {
            if let Stmt::Fun(n, f) = s {
                env.borrow_mut().vars.push((n.clone(), Value::Func(f.clone(), env.clone())));
            }
        }
        for s in stmts {
            self.exec(s, env, doc)?;
        }
        Ok(Value::Undef)
    }

    fn exec(&mut self, s: &Stmt, env: &Env, doc: &mut Doc) -> R {
        self.step()?;
        match s {
            Stmt::Var(list) => {
                for (n, init) in list {
                    let v = match init {
                        Some(e) => self.eval(e, env, doc)?,
                        None => Value::Undef,
                    };
                    env.borrow_mut().vars.push((n.clone(), v));
                }
            }
            Stmt::Expr(e) => {
                self.eval(e, env, doc)?;
            }
            Stmt::If(c, a, b) => {
                if self.eval(c, env, doc)?.truthy() {
                    self.exec(a, env, doc)?;
                } else if let Some(b) = b {
                    self.exec(b, env, doc)?;
                }
            }
            Stmt::While(c, body) => {
                while self.eval(c, env, doc)?.truthy() {
                    match self.exec(body, env, doc) {
                        Err(Ctl::Brk) => break,
                        Err(Ctl::Cont) | Ok(_) => {}
                        Err(e) => return Err(e),
                    }
                }
            }
            Stmt::DoWhile(body, c) => loop {
                match self.exec(body, env, doc) {
                    Err(Ctl::Brk) => break,
                    Err(Ctl::Cont) | Ok(_) => {}
                    Err(e) => return Err(e),
                }
                if !self.eval(c, env, doc)?.truthy() {
                    break;
                }
            },
            Stmt::For(init, cond, upd, body) => {
                let fe = new_env(Some(env.clone()));
                if let Some(i) = init {
                    self.exec(i, &fe, doc)?;
                }
                loop {
                    if let Some(c) = cond {
                        if !self.eval(c, &fe, doc)?.truthy() {
                            break;
                        }
                    }
                    // har aylanişda yangi muhit (yopilmalar uçun let semantikasi)
                    let it = new_env(Some(fe.clone()));
                    match self.exec(body, &it, doc) {
                        Err(Ctl::Brk) => break,
                        Err(Ctl::Cont) | Ok(_) => {}
                        Err(e) => return Err(e),
                    }
                    if let Some(u) = upd {
                        self.eval(u, &fe, doc)?;
                    }
                }
            }
            Stmt::ForOf(n, it, body, of) => {
                let coll = self.eval(it, env, doc)?;
                let items: Vec<Value> = match (&coll, of) {
                    (Value::Arr(a), true) => a.borrow().clone(),
                    (Value::Arr(a), false) => (0..a.borrow().len()).map(|i| Value::str(&num_str(i as f64))).collect(),
                    (Value::Str(s), true) => s.chars().map(|c| Value::str(&c.to_string())).collect(),
                    (Value::Obj(o), false) => o.borrow().iter().map(|(k, _)| Value::str(k)).collect(),
                    (Value::Obj(o), true) => o.borrow().iter().map(|(_, v)| v.clone()).collect(),
                    _ => Vec::new(),
                };
                for v in items {
                    let e2 = new_env(Some(env.clone()));
                    e2.borrow_mut().vars.push((n.clone(), v));
                    match self.exec(body, &e2, doc) {
                        Err(Ctl::Brk) => break,
                        Err(Ctl::Cont) | Ok(_) => {}
                        Err(e) => return Err(e),
                    }
                }
            }
            Stmt::Block(b) => {
                let e2 = new_env(Some(env.clone()));
                self.exec_block(b, &e2, doc)?;
            }
            Stmt::Ret(e) => {
                let v = match e {
                    Some(e) => self.eval(e, env, doc)?,
                    None => Value::Undef,
                };
                return Err(Ctl::Ret(v));
            }
            Stmt::Break => return Err(Ctl::Brk),
            Stmt::Cont => return Err(Ctl::Cont),
            Stmt::Fun(..) | Stmt::Empty => {}
            Stmt::Throw(e) => {
                let v = self.eval(e, env, doc)?;
                return Err(Ctl::Throw(v));
            }
            Stmt::Try(body, catch, fin) => {
                let e2 = new_env(Some(env.clone()));
                let r = self.exec_block(body, &e2, doc);
                let r = match (r, catch) {
                    (Err(Ctl::Throw(v)), Some((n, cb))) => {
                        let e3 = new_env(Some(env.clone()));
                        e3.borrow_mut().vars.push((n.clone(), v));
                        self.exec_block(cb, &e3, doc)
                    }
                    (r, _) => r,
                };
                if !fin.is_empty() {
                    let e4 = new_env(Some(env.clone()));
                    self.exec_block(fin, &e4, doc)?;
                }
                r?;
            }
        }
        Ok(Value::Undef)
    }

    fn call(&mut self, f: &Value, args: Vec<Value>, this: Value, doc: &mut Doc) -> R {
        self.step()?;
        if stack_left() < 10 * 1024 {
            return Err(err("Rekursiya juda çuqur (stek tugadi)"));
        }
        match f {
            Value::Func(def, cenv) => {
                let e = new_env(Some(cenv.clone()));
                {
                    let mut b = e.borrow_mut();
                    for (k, p) in def.params.iter().enumerate() {
                        b.vars.push((p.clone(), args.get(k).cloned().unwrap_or(Value::Undef)));
                    }
                    b.vars.push((String::from("this"), this));
                    b.vars.push((String::from("arguments"), Value::Arr(Rc::new(RefCell::new(args.clone())))));
                }
                let r = if let Some(x) = &def.expr {
                    self.eval(x, &e, doc)
                } else {
                    match self.exec_block(&def.body, &e, doc) {
                        Ok(_) => Ok(Value::Undef),
                        Err(Ctl::Ret(v)) => Ok(v),
                        Err(Ctl::Brk) | Err(Ctl::Cont) => Ok(Value::Undef),
                        Err(e) => Err(e),
                    }
                };
                if def.is_async.get() {
                    // async funksiya doim Promise qaytaradi
                    let p = new_prom();
                    match r {
                        Ok(v) => self.resolve(&p, v),
                        Err(Ctl::Throw(x)) => self.settle(&p, false, x),
                        Err(e) => return Err(e),
                    }
                    return Ok(Value::Promise(p));
                }
                r
            }
            Value::Native(name, recv) => self.native(name, (**recv).clone(), args, doc),
            Value::Host(h) => {
                let a0 = args.first().cloned().unwrap_or(Value::Undef);
                Ok(match *h {
                    "String" => Value::str(&if args.is_empty() { String::new() } else { a0.to_string() }),
                    "Number" => Value::Num(if args.is_empty() { 0.0 } else { a0.num() }),
                    "Boolean" => Value::Bool(a0.truthy()),
                    "Array" => Value::Arr(Rc::new(RefCell::new(args))),
                    "Object" => Value::Obj(Rc::new(RefCell::new(Vec::new()))),
                    "Date" => Value::str(&date_str(self.now_unix_ms(), 0)),
                    _ => return Err(err(&format!("{} — funksiya emas", h))),
                })
            }
            _ => Err(err(&format!("{} — funksiya emas", f.to_string()))),
        }
    }

    fn get(&mut self, obj: &Value, key: &str, doc: &Doc) -> R {
        let nat = |n: &'static str, v: &Value| Value::Native(n, Box::new(v.clone()));
        Ok(match obj {
            Value::Obj(o) => {
                if let Some(v) = o.borrow().iter().find(|(k, _)| k == key).map(|(_, v)| v.clone()) {
                    return Ok(v);
                }
                if o.borrow().iter().any(|(k, _)| k == "__cv") {
                    let m = cv_method(key);
                    if m != "noop" || key == "save" || key == "restore" {
                        return Ok(nat(m, obj));
                    }
                }
                match key {
                    "hasOwnProperty" => nat("obj.has", obj),
                    _ => Value::Undef,
                }
            }
            Value::Arr(a) => {
                if let Some(i) = parse_num(key).filter(|n| *n >= 0.0 && *n == (*n as usize) as f64) {
                    return Ok(a.borrow().get(i as usize).cloned().unwrap_or(Value::Undef));
                }
                match key {
                    "length" => Value::Num(a.borrow().len() as f64),
                    "push" => nat("arr.push", obj),
                    "pop" => nat("arr.pop", obj),
                    "shift" => nat("arr.shift", obj),
                    "unshift" => nat("arr.unshift", obj),
                    "join" => nat("arr.join", obj),
                    "indexOf" => nat("arr.indexOf", obj),
                    "includes" => nat("arr.includes", obj),
                    "slice" => nat("arr.slice", obj),
                    "splice" => nat("arr.splice", obj),
                    "map" => nat("arr.map", obj),
                    "filter" => nat("arr.filter", obj),
                    "forEach" => nat("arr.forEach", obj),
                    "reduce" => nat("arr.reduce", obj),
                    "find" => nat("arr.find", obj),
                    "findIndex" => nat("arr.findIndex", obj),
                    "some" => nat("arr.some", obj),
                    "every" => nat("arr.every", obj),
                    "reverse" => nat("arr.reverse", obj),
                    "sort" => nat("arr.sort", obj),
                    "concat" => nat("arr.concat", obj),
                    "toString" => nat("any.toString", obj),
                    _ => Value::Undef,
                }
            }
            Value::Str(s) => {
                if let Some(i) = parse_num(key).filter(|n| *n >= 0.0) {
                    return Ok(s.chars().nth(i as usize).map(|c| Value::str(&c.to_string())).unwrap_or(Value::Undef));
                }
                match key {
                    "length" => Value::Num(s.chars().count() as f64),
                    "toUpperCase" | "toLowerCase" | "indexOf" | "lastIndexOf" | "includes" | "startsWith" | "endsWith" | "slice" | "substring" | "substr" | "split" | "trim" | "replace" | "replaceAll" | "repeat" | "charAt" | "charCodeAt" | "padStart" | "padEnd" | "concat" | "toString" | "trimStart" | "trimEnd" => {
                        Value::Native(str_method(key), Box::new(obj.clone()))
                    }
                    _ => Value::Undef,
                }
            }
            Value::Num(_) => match key {
                "toFixed" => nat("num.toFixed", obj),
                "toString" => nat("any.toString", obj),
                _ => Value::Undef,
            },
            Value::Promise(_) => match key {
                "then" => nat("prom.then", obj),
                "catch" => nat("prom.catch", obj),
                "finally" => nat("prom.finally", obj),
                _ => Value::Undef,
            },
            Value::Date(_) => match key {
                "getFullYear" | "getMonth" | "getDate" | "getDay" | "getHours" | "getMinutes" | "getSeconds" | "getTime" | "toLocaleTimeString" | "toLocaleDateString" | "toLocaleString" | "toString" | "toISOString" | "getMilliseconds" => {
                    Value::Native(date_method(key), Box::new(obj.clone()))
                }
                _ => Value::Undef,
            },
            Value::Func(..) | Value::Native(..) => match key {
                "call" => nat("fn.call", obj),
                "apply" => nat("fn.apply", obj),
                "bind" => nat("fn.bind", obj),
                _ => Value::Undef,
            },
            Value::Host(h) => match (*h, key) {
                ("Math", "PI") => Value::Num(core::f64::consts::PI),
                ("Math", "E") => Value::Num(core::f64::consts::E),
                ("Math", k) => Value::Native(math_name(k), Box::new(Value::Undef)),
                ("console", _) => Value::Native("console.log", Box::new(Value::Undef)),
                ("document", "getElementById") => Value::Native("doc.byId", Box::new(Value::Undef)),
                ("document", "querySelector") => Value::Native("doc.query", Box::new(Value::Undef)),
                ("document", "querySelectorAll") => Value::Native("doc.queryAll", Box::new(Value::Undef)),
                ("document", "getElementsByClassName") => Value::Native("doc.byClass", Box::new(Value::Undef)),
                ("document", "getElementsByTagName") => Value::Native("doc.byTag", Box::new(Value::Undef)),
                ("document", "createElement") => Value::Native("doc.create", Box::new(Value::Undef)),
                ("document", "cookie") => Value::str(&crate::net::http::cookies_for(&self.url)),
                ("localStorage", k) | ("sessionStorage", k) => match k {
                    "getItem" => Value::Native("ls.get", Box::new(Value::Undef)),
                    "setItem" => Value::Native("ls.set", Box::new(Value::Undef)),
                    "removeItem" => Value::Native("ls.remove", Box::new(Value::Undef)),
                    "clear" => Value::Native("ls.clear", Box::new(Value::Undef)),
                    "length" => Value::Num(ls().iter().filter(|(o, _, _)| *o == origin(&self.url)).count() as f64),
                    key => ls().iter().find(|(o, kk, _)| *o == origin(&self.url) && kk == key).map(|(_, _, v)| Value::str(v)).unwrap_or(Value::Undef),
                },
                ("document", "write") | ("document", "writeln") => Value::Native("doc.write", Box::new(Value::Undef)),
                ("document", "addEventListener") | ("window", "addEventListener") => Value::Native("noop", Box::new(Value::Undef)),
                ("document", "title") => Value::str(&doc.title),
                ("document", "body") | ("document", "documentElement") => Value::Host("body"),
                ("window", "document") => Value::Host("document"),
                ("window", "innerWidth") => Value::Num(gfx_w() as f64),
                ("window", "innerHeight") => Value::Num(gfx_h() as f64),
                ("window", k) => self.lookup(&self.globals.clone(), k).unwrap_or(Value::Undef),
                ("JSON", "stringify") => Value::Native("json.stringify", Box::new(Value::Undef)),
                ("JSON", "parse") => Value::Native("json.parse", Box::new(Value::Undef)),
                ("Promise", "resolve") => Value::Native("Promise.resolve", Box::new(Value::Undef)),
                ("Promise", "reject") => Value::Native("Promise.reject", Box::new(Value::Undef)),
                ("Promise", "all") => Value::Native("Promise.all", Box::new(Value::Undef)),
                ("Object", "keys") => Value::Native("obj.keys", Box::new(Value::Undef)),
                ("Object", "values") => Value::Native("obj.values", Box::new(Value::Undef)),
                ("Object", "entries") => Value::Native("obj.entries", Box::new(Value::Undef)),
                ("Array", "isArray") => Value::Native("arr.isArray", Box::new(Value::Undef)),
                ("Array", "from") => Value::Native("arr.from", Box::new(Value::Undef)),
                ("Date", "now") => Value::Native("date.now", Box::new(Value::Undef)),
                ("Number", "isInteger") => Value::Native("num.isInteger", Box::new(Value::Undef)),
                ("Number", "parseFloat") => Value::Native("parseFloat", Box::new(Value::Undef)),
                ("Number", "parseInt") => Value::Native("parseInt", Box::new(Value::Undef)),
                ("navigator", "userAgent") => Value::str(concat!("ZaminBrauzer/", env!("CARGO_PKG_VERSION"), " (Zamin OS; ARM64)")),
                ("navigator", "language") => Value::str("uz"),
                ("location", "href") | ("document", "URL") => Value::str(&self.url),
                ("location", "search") => Value::str(self.url.find('?').map(|i| &self.url[i..]).unwrap_or("").split('#').next().unwrap_or("")),
                ("location", "hash") => Value::str(self.url.find('#').map(|i| &self.url[i..]).unwrap_or("")),
                ("location", "host") | ("location", "hostname") => Value::str(self.url.split("://").nth(1).and_then(|r| r.split('/').next()).unwrap_or("")),
                ("location", "pathname") => Value::str(&format!("/{}", self.url.split("://").nth(1).and_then(|r| r.split_once('/')).map(|(_, p)| p.split('?').next().unwrap_or("")).unwrap_or(""))),
                ("location", "protocol") => Value::str(&format!("{}:", self.url.split("://").next().unwrap_or(""))),
                ("body", "style") => Value::Style(usize::MAX),
                ("body", "appendChild") | ("body", "append") | ("body", "insertAdjacentHTML") => {
                    let b = doc.elems.iter().position(|e| e.tag == "body").unwrap_or(0);
                    Value::Native(if key == "insertAdjacentHTML" { "el.insertHTML" } else { "el.append" }, Box::new(Value::Elem(b)))
                }
                _ => Value::Undef,
            },
            Value::Elem(i) => match key {
                "textContent" | "innerText" | "innerHTML" => Value::str(&doc.text_of(*i)),
                "className" => Value::str(&doc.elems.get(*i).map(|e| e.classes.join(" ")).unwrap_or_default()),
                "classList" => Value::Obj(Rc::new(RefCell::new(alloc::vec![
                    (String::from("add"), nat("cl.add", obj)),
                    (String::from("remove"), nat("cl.remove", obj)),
                    (String::from("toggle"), nat("cl.toggle", obj)),
                    (String::from("contains"), nat("cl.contains", obj)),
                ]))),
                "parentElement" | "parentNode" => match doc.elems.get(*i).map(|e| e.parent) {
                    Some(p) if p != crate::html::NO_EL => Value::Elem(p as usize),
                    _ => Value::Null,
                },
                "children" => Value::Arr(Rc::new(RefCell::new(
                    (0..doc.elems.len()).filter(|&k| doc.elems[k].parent as usize == *i).map(Value::Elem).collect(),
                ))),
                "appendChild" | "append" => nat("el.append", obj),
                "insertAdjacentHTML" => nat("el.insertHTML", obj),
                "remove" => nat("el.remove", obj),
                "querySelector" => nat("doc.query", obj),
                "querySelectorAll" => nat("doc.queryAll", obj),
                "setAttribute" => nat("el.setAttr", obj),
                "focus" | "blur" | "click" | "scrollIntoView" => nat("noop", obj),
                "id" => Value::str(&doc.elems.get(*i).map(|e| e.id.clone()).unwrap_or_default()),
                "tagName" => Value::str(&doc.elems.get(*i).map(|e| e.tag.to_ascii_uppercase()).unwrap_or_default()),
                "style" => Value::Style(*i),
                "addEventListener" => nat("el.listen", obj),
                "getContext" => nat("el.getContext", obj),
                "width" | "height" if doc.canvases.iter().any(|c| c.el as usize == *i) => {
                    let c = doc.canvases.iter().find(|c| c.el as usize == *i).unwrap();
                    Value::Num(if key == "width" { c.w } else { c.h } as f64)
                }
                "value" => Value::str(&doc.input_of(*i).map(|k| doc.inputs[k].value.clone()).unwrap_or_default()),
                "checked" => Value::Bool(doc.input_of(*i).is_some_and(|k| doc.inputs[k].checked)),
                _ => Value::Undef,
            },
            _ => Value::Undef,
        })
    }

    fn put(&mut self, obj: &Value, key: &str, v: Value, doc: &mut Doc) -> Result<(), Ctl> {
        match obj {
            Value::Obj(o) => {
                let mut b = o.borrow_mut();
                match b.iter_mut().find(|(k, _)| k == key) {
                    Some(slot) => slot.1 = v,
                    None => b.push((String::from(key), v)),
                }
            }
            Value::Arr(a) => {
                if key == "length" {
                    a.borrow_mut().truncate(v.num() as usize);
                } else if let Some(i) = parse_num(key) {
                    let i = i as usize;
                    let mut b = a.borrow_mut();
                    if i > 100_000 {
                        return Err(err("massiv juda katta"));
                    }
                    while b.len() <= i {
                        b.push(Value::Undef);
                    }
                    b[i] = v;
                }
            }
            Value::Elem(i) => match key {
                "textContent" | "innerText" => doc.set_content(*i, &v.to_string(), false),
                "innerHTML" => doc.set_content(*i, &v.to_string(), true),
                "className" => {
                    if let Some(e) = doc.elems.get_mut(*i) {
                        e.classes = v.to_string().split_whitespace().map(String::from).collect();
                    }
                    doc.restyle(*i);
                }
                "hidden" => doc.set_style(*i, "display", if v.truthy() { "none" } else { "block" }),
                "width" | "height" => {
                    let n = v.num();
                    let n = if n.is_finite() && n >= 0.0 { n as usize } else { 0 };
                    if let Some(k) = doc.canvases.iter().position(|c| c.el as usize == *i) {
                        let (w, h) = if key == "width" { (n, doc.canvases[k].h) } else { (doc.canvases[k].w, n) };
                        doc.canvases[k].resize(w, h);
                        let (w, h) = (doc.canvases[k].w as u16, doc.canvases[k].h as u16);
                        let src = format!("canvas:{}", k);
                        for b in doc.blocks.iter_mut().filter(|b| b.src == src) {
                            b.iw = w;
                            b.ih = h;
                        }
                    }
                }
                "value" => {
                    if let Some(k) = doc.input_of(*i) {
                        doc.inputs[k].value = v.to_string();
                    }
                }
                "checked" => {
                    if let Some(k) = doc.input_of(*i) {
                        doc.inputs[k].checked = v.truthy();
                    }
                }
                "onsubmit" => {
                    self.submit_handlers.retain(|(e, _)| e != i);
                    self.submit_handlers.push((*i, v));
                }
                "onclick" => {
                    self.handlers.retain(|(e, _)| e != i);
                    self.handlers.push((*i, v));
                }
                _ => {}
            },
            Value::Style(i) => {
                let s = v.to_string();
                if *i == usize::MAX {
                    if key == "background" || key == "backgroundColor" {
                        doc.bg = crate::css::parse_color(&s);
                    } else if key == "color" {
                        doc.color = crate::css::parse_color(&s);
                    }
                } else {
                    doc.set_style(*i, key, &s);
                }
            }
            Value::Host("document") if key == "title" => doc.title = v.to_string(),
            Value::Host("document") if key == "cookie" => crate::net::http::set_cookie(&self.url, &v.to_string()),
            Value::Host("localStorage") | Value::Host("sessionStorage") => ls_set(&self.url, key, &v.to_string()),
            Value::Host("window") => self.set_var(&self.globals.clone(), key, v),
            _ => {}
        }
        Ok(())
    }

    fn eval(&mut self, e: &Expr, env: &Env, doc: &mut Doc) -> R {
        self.step()?;
        Ok(match e {
            Expr::Num(n) => Value::Num(*n),
            Expr::Str(s) => Value::Str(s.clone()),
            Expr::Bool(b) => Value::Bool(*b),
            Expr::Null => Value::Null,
            Expr::Undef => Value::Undef,
            Expr::Tmpl(parts) => {
                let mut s = String::new();
                for p in parts {
                    match p {
                        TPart::S(t) => s.push_str(t),
                        TPart::E(x) => s.push_str(&self.eval(x, env, doc)?.to_string()),
                    }
                }
                Value::str(&s)
            }
            Expr::Id(n) => match self.lookup(env, n) {
                Some(v) => v,
                None => return Err(err(&format!("{} aniqlanmagan", n))),
            },
            Expr::Arr(items) => Value::Arr(Rc::new(RefCell::new(self.eval_list(items, env, doc)?))),
            Expr::Spread(x) => self.eval(x, env, doc)?,
            Expr::Await(x) => {
                let v = self.eval(x, env, doc)?;
                let Value::Promise(p) = &v else { return Ok(v) };
                // Kutiş: fetch so'rovlarini va taymerlarni joyida bajarib, promise yeçilgunça
                let deadline = crate::timer::uptime_ms() + 20_000;
                loop {
                    self.drain(doc);
                    let st = p.borrow().state.clone();
                    match st {
                        PState::Ok(v) => break v,
                        PState::Err(e) => return Err(Ctl::Throw(e)),
                        PState::Pending => {}
                    }
                    if crate::timer::uptime_ms() > deadline || !self.pump_sync(doc) {
                        return Err(err("await: Promise yeçilmadi (vaqt tugadi yoki kutiladigan hodisa yöq)"));
                    }
                }
            }
            Expr::Obj(props) => {
                let mut v = Vec::new();
                for (k, x) in props {
                    v.push((k.clone(), self.eval(x, env, doc)?));
                }
                Value::Obj(Rc::new(RefCell::new(v)))
            }
            Expr::Fun(f) => Value::Func(f.clone(), env.clone()),
            Expr::Un(op, x) => {
                if *op == "typeof" {
                    if let Expr::Id(n) = &**x {
                        return Ok(Value::str(self.lookup(env, n).map(|v| v.type_of()).unwrap_or("undefined")));
                    }
                }
                let v = self.eval(x, env, doc)?;
                match *op {
                    "!" => Value::Bool(!v.truthy()),
                    "-" => Value::Num(-v.num()),
                    "+" => Value::Num(v.num()),
                    "~" => Value::Num(!(v.num() as i32) as f64),
                    "typeof" => Value::str(v.type_of()),
                    _ => Value::Undef,
                }
            }
            Expr::Bin(op, a, b) => {
                match *op {
                    "&&" => {
                        let l = self.eval(a, env, doc)?;
                        return if l.truthy() { self.eval(b, env, doc) } else { Ok(l) };
                    }
                    "||" => {
                        let l = self.eval(a, env, doc)?;
                        return if l.truthy() { Ok(l) } else { self.eval(b, env, doc) };
                    }
                    "??" => {
                        let l = self.eval(a, env, doc)?;
                        return if matches!(l, Value::Undef | Value::Null) { self.eval(b, env, doc) } else { Ok(l) };
                    }
                    _ => {}
                }
                let l = self.eval(a, env, doc)?;
                let r = self.eval(b, env, doc)?;
                binop(op, &l, &r)
            }
            Expr::Cond(c, a, b) => {
                if self.eval(c, env, doc)?.truthy() {
                    self.eval(a, env, doc)?
                } else {
                    self.eval(b, env, doc)?
                }
            }
            Expr::Assign(op, target, rhs) => {
                let mut v = self.eval(rhs, env, doc)?;
                if *op != "=" {
                    let old = self.eval(target, env, doc)?;
                    v = binop(&op[..op.len() - 1], &old, &v);
                }
                self.assign_to(target, v.clone(), env, doc)?;
                v
            }
            Expr::Upd(prefix, inc, target) => {
                let old = self.eval(target, env, doc)?.num();
                let new = if *inc { old + 1.0 } else { old - 1.0 };
                self.assign_to(target, Value::Num(new), env, doc)?;
                Value::Num(if *prefix { new } else { old })
            }
            Expr::Mem(o, k) => {
                let ov = self.eval(o, env, doc)?;
                if matches!(ov, Value::Undef | Value::Null) {
                    return Err(err(&format!("{} ning '{}' xossasini öqib bölmaydi", ov.to_string(), k)));
                }
                self.get(&ov, k, doc)?
            }
            Expr::Idx(o, k) => {
                let ov = self.eval(o, env, doc)?;
                let kv = self.eval(k, env, doc)?.to_string();
                self.get(&ov, &kv, doc)?
            }
            Expr::Call(callee, args) => {
                let (f, this) = match &**callee {
                    Expr::Mem(o, k) => {
                        let ov = self.eval(o, env, doc)?;
                        let f = self.get(&ov, k, doc)?;
                        (f, ov)
                    }
                    Expr::Idx(o, k) => {
                        let ov = self.eval(o, env, doc)?;
                        let kv = self.eval(k, env, doc)?.to_string();
                        (self.get(&ov, &kv, doc)?, ov)
                    }
                    c => (self.eval(c, env, doc)?, Value::Undef),
                };
                let av = self.eval_list(args, env, doc)?;
                if matches!(f, Value::Undef) {
                    return Err(err("funksiya emas (aniqlanmagan)"));
                }
                self.call(&f, av, this, doc)?
            }
            Expr::New(c, args) => {
                let av = self.eval_list(args, env, doc)?;
                match &**c {
                    Expr::Id(n) if n == "Date" => Value::Date(match av.first() {
                        Some(v) => v.num(),
                        None => self.now_unix_ms(),
                    }),
                    Expr::Id(n) if n == "Promise" => {
                        let p = new_prom();
                        let res = Value::Native("prom.res", Box::new(Value::Promise(p.clone())));
                        let rej = Value::Native("prom.rej", Box::new(Value::Promise(p.clone())));
                        if let Some(f) = av.first() {
                            if let Err(Ctl::Throw(e)) = self.call(f, alloc::vec![res, rej], Value::Undef, doc) {
                                self.settle(&p, false, e);
                            }
                        }
                        Value::Promise(p)
                    }
                    Expr::Id(n) if n == "URLSearchParams" => {
                        let q = av.first().map(|v| v.to_string()).unwrap_or_default();
                        let mut o = Vec::new();
                        for pair in q.trim_start_matches('?').split('&').filter(|x| !x.is_empty()) {
                            let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
                            o.push((url_decode(k), Value::str(&url_decode(v))));
                        }
                        let obj = Value::Obj(Rc::new(RefCell::new(o)));
                        Value::Obj(Rc::new(RefCell::new(alloc::vec![
                            (String::from("get"), Value::Native("usp.get", Box::new(obj.clone()))),
                            (String::from("has"), Value::Native("usp.has", Box::new(obj))),
                        ])))
                    }
                    Expr::Id(n) if n == "Array" => Value::Arr(Rc::new(RefCell::new(alloc::vec![Value::Undef; av.first().map(|v| v.num() as usize).unwrap_or(0).min(10000)]))),
                    Expr::Id(n) if n == "Object" => Value::Obj(Rc::new(RefCell::new(Vec::new()))),
                    Expr::Id(n) if n == "Error" => {
                        let msg = av.first().map(|v| v.to_string()).unwrap_or_default();
                        Value::Obj(Rc::new(RefCell::new(alloc::vec![(String::from("message"), Value::str(&msg))])))
                    }
                    _ => {
                        // Oddiy konstruktor funksiya: this — yangi obyekt
                        let f = self.eval(c, env, doc)?;
                        let o = Value::Obj(Rc::new(RefCell::new(Vec::new())));
                        let r = self.call(&f, av, o.clone(), doc)?;
                        if matches!(r, Value::Obj(_)) { r } else { o }
                    }
                }
            }
        })
    }

    /// Ifodalar ro'yxati (...spread yoyiladi).
    fn eval_list(&mut self, xs: &[Expr], env: &Env, doc: &mut Doc) -> Result<Vec<Value>, Ctl> {
        let mut v = Vec::new();
        for x in xs {
            match x {
                Expr::Spread(inner) => match self.eval(inner, env, doc)? {
                    Value::Arr(a) => v.extend(a.borrow().iter().cloned()),
                    Value::Str(s) => v.extend(s.chars().map(|c| Value::str(&c.to_string()))),
                    o => v.push(o),
                },
                x => v.push(self.eval(x, env, doc)?),
            }
        }
        Ok(v)
    }

    fn now_unix_ms(&self) -> f64 {
        crate::timer::unix_time() as f64 * 1000.0 + (crate::timer::uptime_ms() % 1000) as f64
    }

    fn assign_to(&mut self, target: &Expr, v: Value, env: &Env, doc: &mut Doc) -> Result<(), Ctl> {
        match target {
            Expr::Id(n) => {
                self.set_var(env, n, v);
                Ok(())
            }
            Expr::Mem(o, k) => {
                let ov = self.eval(o, env, doc)?;
                self.put(&ov, k, v, doc)
            }
            Expr::Idx(o, k) => {
                let ov = self.eval(o, env, doc)?;
                let kv = self.eval(k, env, doc)?.to_string();
                self.put(&ov, &kv, v, doc)
            }
            _ => Err(err("notöğri tayinlaş")),
        }
    }

    fn native(&mut self, name: &str, this: Value, a: Vec<Value>, doc: &mut Doc) -> R {
        let arg = |k: usize| a.get(k).cloned().unwrap_or(Value::Undef);
        let arr_of = |v: &Value| match v {
            Value::Arr(x) => x.clone(),
            _ => Rc::new(RefCell::new(Vec::new())),
        };
        let new_arr = |v: Vec<Value>| Value::Arr(Rc::new(RefCell::new(v)));
        Ok(match name {
            "noop" => Value::Undef,
            "ev.prevent" => {
                self.prevented = true;
                Value::Undef
            }
            "fetch" => {
                let id = self.next_id;
                self.next_id += 1;
                let p = new_prom();
                let url = arg(0).to_string();
                let (mut method, mut body) = (String::from("GET"), None);
                if let Value::Obj(o) = arg(1) {
                    for (k, v) in o.borrow().iter() {
                        match k.as_str() {
                            "method" => method = v.to_string().to_ascii_uppercase(),
                            "body" => body = Some(v.to_string()),
                            _ => {}
                        }
                    }
                }
                if self.fetch_out.len() + self.fetch_wait.len() < 16 {
                    self.fetch_out.push((id, method, url, body));
                    self.fetch_wait.push((id, p.clone()));
                } else {
                    self.settle(&p, false, Value::str("TypeError: fetch: juda köp sörov"));
                }
                Value::Promise(p)
            }
            "resp.text" => {
                let p = new_prom();
                self.settle(&p, true, this.clone());
                Value::Promise(p)
            }
            "resp.json" => {
                let p = new_prom();
                match json_parse(&this.to_string()) {
                    Some(v) => self.settle(&p, true, v),
                    None => self.settle(&p, false, Value::str("SyntaxError: JSON xato")),
                }
                Value::Promise(p)
            }
            "json.parse" => match json_parse(&arg(0).to_string()) {
                Some(v) => v,
                None => return Err(err("SyntaxError: JSON.parse: notöğri JSON")),
            },
            "prom.then" | "prom.catch" | "prom.finally" => {
                let Value::Promise(p) = &this else { return Ok(Value::Undef) };
                let (fo, fe) = match name {
                    "prom.then" => (arg(0), arg(1)),
                    "prom.catch" => (Value::Undef, arg(0)),
                    _ => (arg(0), arg(0)),
                };
                self.then(p, fo, fe)
            }
            "prom.res" | "prom.rej" => {
                if let Value::Promise(p) = &this {
                    if name == "prom.res" {
                        self.resolve(p, arg(0));
                    } else {
                        self.settle(p, false, arg(0));
                    }
                }
                Value::Undef
            }
            "Promise.resolve" | "Promise.reject" => {
                let p = new_prom();
                if name == "Promise.resolve" {
                    self.resolve(&p, arg(0));
                } else {
                    self.settle(&p, false, arg(0));
                }
                Value::Promise(p)
            }
            "Promise.all" => {
                // Faqat allaqaçon yeçilganlar uçun sodda variant
                let items = match arg(0) {
                    Value::Arr(x) => x.borrow().clone(),
                    _ => Vec::new(),
                };
                let p = new_prom();
                let mut out = Vec::new();
                let mut bad = None;
                for it in items {
                    match &it {
                        Value::Promise(q) => match q.borrow().state.clone() {
                            PState::Ok(v) => out.push(v),
                            PState::Err(e) => bad = Some(e),
                            PState::Pending => out.push(Value::Undef),
                        },
                        v => out.push(v.clone()),
                    }
                }
                match bad {
                    Some(e) => self.settle(&p, false, e),
                    None => self.settle(&p, true, Value::Arr(Rc::new(RefCell::new(out)))),
                }
                Value::Promise(p)
            }
            "encodeURIComponent" | "encodeURI" => Value::str(&url_encode(&arg(0).to_string(), name == "encodeURI")),
            "decodeURIComponent" | "decodeURI" => Value::str(&url_decode(&arg(0).to_string())),
            "usp.get" | "usp.has" => {
                let key = arg(0).to_string();
                let v = match &this {
                    Value::Obj(o) => o.borrow().iter().find(|(k, _)| *k == key).map(|(_, v)| v.clone()),
                    _ => None,
                };
                if name == "usp.has" { Value::Bool(v.is_some()) } else { v.unwrap_or(Value::Null) }
            }
            "__sl" => Value::Num(stack_left() as f64),
            "console.log" => {
                let s = a.iter().map(|v| match v {
                    Value::Arr(_) | Value::Obj(_) => json(v, 0),
                    _ => v.to_string(),
                }).collect::<Vec<_>>().join(" ");
                crate::println!("[js] {}", s);
                self.log.push(s);
                if self.log.len() > 200 {
                    self.log.remove(0);
                }
                Value::Undef
            }
            "alert" => {
                self.alerts.push(arg(0).to_string());
                Value::Undef
            }
            "confirm" => Value::Bool(true),
            "prompt" => arg(1),
            "setTimeout" | "setInterval" => {
                let ms = arg(1).num();
                let ms = if ms.is_nan() { 0 } else { ms.max(0.0) as u64 };
                let id = self.next_id;
                self.next_id += 1;
                if self.timers.len() < 64 {
                    self.timers.push(Timer { at: self.now + ms, every: if name == "setInterval" { Some(ms) } else { None }, f: arg(0), id });
                }
                Value::Num(id as f64)
            }
            "clearTimeout" | "clearInterval" => {
                let id = arg(0).num() as u32;
                self.timers.retain(|t| t.id != id);
                Value::Undef
            }
            "parseInt" => {
                let s = arg(0).to_string();
                let t = s.trim();
                let radix = if matches!(arg(1), Value::Undef) { 10 } else { arg(1).num() as u32 };
                let (neg, t) = match t.strip_prefix('-') { Some(r) => (true, r), None => (false, t) };
                let t = if radix == 16 { t.trim_start_matches("0x") } else { t };
                let digits: String = t.chars().take_while(|c| c.is_digit(radix.clamp(2, 36))).collect();
                match u64::from_str_radix(&digits, radix.clamp(2, 36)) {
                    Ok(v) => Value::Num(if neg { -(v as f64) } else { v as f64 }),
                    Err(_) => Value::Num(f64::NAN),
                }
            }
            "parseFloat" => {
                let s = arg(0).to_string();
                let t = s.trim();
                let end = t.char_indices().find(|(k, c)| !(c.is_ascii_digit() || *c == '.' || (*k == 0 && (*c == '-' || *c == '+')))).map(|(k, _)| k).unwrap_or(t.len());
                Value::Num(parse_num(&t[..end]).unwrap_or(f64::NAN))
            }
            "isNaN" => Value::Bool(arg(0).num().is_nan()),
            "isFinite" => Value::Bool(arg(0).num().is_finite()),
            "num.isInteger" => Value::Bool({
                let n = arg(0).num();
                n.is_finite() && n == (n as i64) as f64
            }),
            "num.toFixed" => {
                let d = arg(0).num().clamp(0.0, 20.0) as usize;
                Value::str(&format!("{:.*}", d, this.num()))
            }
            "any.toString" => Value::str(&this.to_string()),
            "json.stringify" => Value::str(&json(&arg(0), 0)),
            "obj.keys" | "obj.values" | "obj.entries" => match arg(0) {
                Value::Obj(o) => new_arr(o.borrow().iter().map(|(k, v)| match name {
                    "obj.keys" => Value::str(k),
                    "obj.values" => v.clone(),
                    _ => new_arr(alloc::vec![Value::str(k), v.clone()]),
                }).collect()),
                Value::Arr(x) => new_arr((0..x.borrow().len()).map(|i| Value::str(&num_str(i as f64))).collect()),
                _ => new_arr(Vec::new()),
            },
            "obj.has" => Value::Bool(matches!(&this, Value::Obj(o) if o.borrow().iter().any(|(k, _)| *k == arg(0).to_string()))),
            "arr.isArray" => Value::Bool(matches!(arg(0), Value::Arr(_))),
            "arr.from" => match arg(0) {
                Value::Arr(x) => new_arr(x.borrow().clone()),
                Value::Str(s) => new_arr(s.chars().map(|c| Value::str(&c.to_string())).collect()),
                Value::Obj(o) => {
                    let n = o.borrow().iter().find(|(k, _)| k == "length").map(|(_, v)| v.num() as usize).unwrap_or(0).min(10000);
                    let f = arg(1);
                    let mut v = Vec::new();
                    for i in 0..n {
                        v.push(if matches!(f, Value::Undef) { Value::Undef } else { self.call(&f, alloc::vec![Value::Undef, Value::Num(i as f64)], Value::Undef, doc)? });
                    }
                    new_arr(v)
                }
                _ => new_arr(Vec::new()),
            },
            "date.now" => Value::Num(self.now_unix_ms()),
            "fn.call" => {
                let mut rest = a.clone();
                let t = if rest.is_empty() { Value::Undef } else { rest.remove(0) };
                self.call(&this, rest, t, doc)?
            }
            "fn.apply" => {
                let args = match arg(1) {
                    Value::Arr(x) => x.borrow().clone(),
                    _ => Vec::new(),
                };
                self.call(&this, args, arg(0), doc)?
            }
            "fn.bind" => this.clone(),
            "doc.byId" => match doc.find_id(&arg(0).to_string()) {
                Some(i) => Value::Elem(i),
                None => Value::Null,
            },
            "doc.query" | "doc.queryAll" | "doc.byClass" | "doc.byTag" => {
                let s = arg(0).to_string();
                let sel = match name {
                    "doc.byClass" => format!(".{}", s),
                    _ => s.clone(),
                };
                let mut found: Vec<usize> = match sel.strip_prefix('#').and_then(|id| doc.find_id(id)) {
                    Some(i) if !sel.contains(['.', ' ']) => alloc::vec![i],
                    _ => doc.query_all(&sel),
                };
                // element.querySelector — faqat uning avlodlari
                if let Value::Elem(root) = this {
                    found.retain(|&k| k != root && doc.within(k as u16, root as u16));
                }
                if name == "doc.query" {
                    found.first().map(|&i| Value::Elem(i)).unwrap_or(Value::Null)
                } else {
                    Value::Arr(Rc::new(RefCell::new(found.into_iter().map(Value::Elem).collect())))
                }
            }
            "doc.create" => Value::Obj(Rc::new(RefCell::new(alloc::vec![
                (String::from("__tag"), Value::str(&arg(0).to_string().to_ascii_lowercase())),
                (String::from("textContent"), Value::str("")),
                (String::from("innerHTML"), Value::str("")),
                (String::from("className"), Value::str("")),
                (String::from("id"), Value::str("")),
            ]))),
            "el.append" | "el.insertHTML" => {
                let Value::Elem(target) = this else { return Ok(Value::Undef) };
                let html = if name == "el.insertHTML" {
                    arg(1).to_string()
                } else {
                    match arg(0) {
                        Value::Obj(o) => {
                            let g = |k: &str| o.borrow().iter().find(|(x, _)| x == k).map(|(_, v)| v.to_string()).unwrap_or_default();
                            let tag = if g("__tag").is_empty() { String::from("div") } else { g("__tag") };
                            let inner = if g("innerHTML").is_empty() {
                                g("textContent").replace('&', "&amp;").replace('<', "&lt;")
                            } else {
                                g("innerHTML")
                            };
                            let mut attrs = String::new();
                            if !g("id").is_empty() {
                                attrs.push_str(&format!(" id=\"{}\"", g("id")));
                            }
                            if !g("className").is_empty() {
                                attrs.push_str(&format!(" class=\"{}\"", g("className")));
                            }
                            format!("<{}{}>{}</{}>", tag, attrs, inner, tag)
                        }
                        Value::Elem(src) => format!("<p>{}</p>", doc.text_of(src).replace('<', "&lt;")),
                        v => v.to_string().replace('<', "&lt;"),
                    }
                };
                doc.append_into(target, &html);
                Value::Undef
            }
            "el.remove" => {
                if let Value::Elem(i) = this {
                    doc.set_style(i, "display", "none");
                }
                Value::Undef
            }
            "el.getContext" => {
                let Value::Elem(i) = this else { return Ok(Value::Null) };
                let kind = arg(0).to_string();
                if kind != "2d" {
                    return Ok(Value::Null);
                }
                let Some(n) = doc.canvases.iter().position(|c| c.el as usize == i) else { return Ok(Value::Null) };
                if let Some((_, v)) = self.ctxs.iter().find(|(k, _)| *k == n) {
                    return Ok(v.clone());
                }
                let st = |v: &str| Value::str(v);
                let ctx = Value::Obj(Rc::new(RefCell::new(alloc::vec![
                    (String::from("__cv"), Value::Num(n as f64)),
                    (String::from("canvas"), Value::Elem(i)),
                    (String::from("fillStyle"), st("#000000")),
                    (String::from("strokeStyle"), st("#000000")),
                    (String::from("lineWidth"), Value::Num(1.0)),
                    (String::from("font"), st("10px sans-serif")),
                    (String::from("textAlign"), st("start")),
                    (String::from("textBaseline"), st("alphabetic")),
                    (String::from("globalAlpha"), Value::Num(1.0)),
                ])));
                self.ctxs.push((n, ctx.clone()));
                ctx
            }
            n if n.starts_with("cv.") => return Ok(canvas_call(n, &this, &a, doc)),
            "el.setAttr" => {
                if let Value::Elem(i) = this {
                    match arg(0).to_string().as_str() {
                        "class" => {
                            if let Some(e) = doc.elems.get_mut(i) {
                                e.classes = arg(1).to_string().split_whitespace().map(String::from).collect();
                            }
                            doc.restyle(i);
                        }
                        "style" => {
                            for (k, v) in crate::css::parse_decls(&arg(1).to_string()) {
                                doc.set_style(i, &k, &v);
                            }
                        }
                        _ => {}
                    }
                }
                Value::Undef
            }
            "cl.add" | "cl.remove" | "cl.toggle" | "cl.contains" => {
                let Value::Elem(i) = this else { return Ok(Value::Undef) };
                let c = arg(0).to_string();
                let Some(e) = doc.elems.get_mut(i) else { return Ok(Value::Undef) };
                let has = e.classes.contains(&c);
                let r = match name {
                    "cl.contains" => return Ok(Value::Bool(has)),
                    "cl.add" => {
                        if !has {
                            e.classes.push(c);
                        }
                        true
                    }
                    "cl.remove" => {
                        e.classes.retain(|x| *x != c);
                        false
                    }
                    _ => {
                        if has {
                            e.classes.retain(|x| *x != c);
                        } else {
                            e.classes.push(c);
                        }
                        !has
                    }
                };
                doc.restyle(i);
                Value::Bool(r)
            }
            "ls.get" => ls().iter().find(|(o, k, _)| *o == origin(&self.url) && *k == arg(0).to_string()).map(|(_, _, v)| Value::str(v)).unwrap_or(Value::Null),
            "ls.set" => {
                ls_set(&self.url, &arg(0).to_string(), &arg(1).to_string());
                Value::Undef
            }
            "ls.remove" => {
                let (o, k) = (origin(&self.url), arg(0).to_string());
                ls().retain(|(oo, kk, _)| !(*oo == o && *kk == k));
                ls_save();
                Value::Undef
            }
            "ls.clear" => {
                let o = origin(&self.url);
                ls().retain(|(oo, _, _)| *oo != o);
                ls_save();
                Value::Undef
            }
            "doc.write" => {
                let s: String = a.iter().map(|v| v.to_string()).collect();
                doc.append_html(&s);
                Value::Undef
            }
            "el.listen" => {
                if let Value::Elem(i) = this {
                    match arg(0).to_string().as_str() {
                        "click" => self.handlers.push((i, arg(1))),
                        "submit" => self.submit_handlers.push((i, arg(1))),
                        _ => {}
                    }
                }
                Value::Undef
            }
            n if n.starts_with("Math.") => math(n, &a),
            n if n.starts_with("str.") => str_call(n, &this, &a),
            n if n.starts_with("date.") => date_call(n, &this),
            n if n.starts_with("arr.") => {
                let x = arr_of(&this);
                match n {
                    "arr.push" => {
                        x.borrow_mut().extend(a.iter().cloned());
                        Value::Num(x.borrow().len() as f64)
                    }
                    "arr.pop" => x.borrow_mut().pop().unwrap_or(Value::Undef),
                    "arr.shift" => {
                        let mut b = x.borrow_mut();
                        if b.is_empty() { Value::Undef } else { b.remove(0) }
                    }
                    "arr.unshift" => {
                        let mut b = x.borrow_mut();
                        for (k, v) in a.iter().enumerate() {
                            b.insert(k, v.clone());
                        }
                        Value::Num(b.len() as f64)
                    }
                    "arr.join" => {
                        let sep = if matches!(arg(0), Value::Undef) { String::from(",") } else { arg(0).to_string() };
                        Value::str(&x.borrow().iter().map(|v| if matches!(v, Value::Undef | Value::Null) { String::new() } else { v.to_string() }).collect::<Vec<_>>().join(&sep))
                    }
                    "arr.indexOf" | "arr.includes" => {
                        let t = arg(0);
                        let p = x.borrow().iter().position(|v| strict_eq(v, &t));
                        if n == "arr.includes" { Value::Bool(p.is_some()) } else { Value::Num(p.map(|p| p as f64).unwrap_or(-1.0)) }
                    }
                    "arr.slice" => {
                        let b = x.borrow();
                        let (s, e) = slice_range(b.len(), &arg(0), &arg(1));
                        new_arr(b[s..e].to_vec())
                    }
                    "arr.splice" => {
                        let mut b = x.borrow_mut();
                        let len = b.len();
                        let (s, _) = slice_range(len, &arg(0), &Value::Undef);
                        let cnt = if matches!(arg(1), Value::Undef) { len - s } else { (arg(1).num().max(0.0) as usize).min(len - s) };
                        let removed: Vec<Value> = b.drain(s..s + cnt).collect();
                        for (k, v) in a.iter().skip(2).enumerate() {
                            b.insert(s + k, v.clone());
                        }
                        new_arr(removed)
                    }
                    "arr.reverse" => {
                        x.borrow_mut().reverse();
                        this.clone()
                    }
                    "arr.concat" => {
                        let mut v = x.borrow().clone();
                        for o in &a {
                            match o {
                                Value::Arr(y) => v.extend(y.borrow().iter().cloned()),
                                o => v.push(o.clone()),
                            }
                        }
                        new_arr(v)
                    }
                    "arr.sort" => {
                        let mut v = x.borrow().clone();
                        let f = arg(0);
                        // qöşib saralaş (barqaror), solişturgiç bilan yoki satr sifatida
                        for i in 1..v.len() {
                            let mut j = i;
                            while j > 0 {
                                let gt = if matches!(f, Value::Undef) {
                                    v[j - 1].to_string() > v[j].to_string()
                                } else {
                                    self.call(&f, alloc::vec![v[j - 1].clone(), v[j].clone()], Value::Undef, doc)?.num() > 0.0
                                };
                                if !gt {
                                    break;
                                }
                                v.swap(j - 1, j);
                                j -= 1;
                            }
                        }
                        *x.borrow_mut() = v;
                        this.clone()
                    }
                    _ => {
                        // Funksiya qabul qiluvçilar
                        let f = arg(0);
                        let items = x.borrow().clone();
                        let mut acc = if n == "arr.reduce" {
                            if a.len() > 1 { Some(arg(1)) } else { None }
                        } else {
                            None
                        };
                        let mut out = Vec::new();
                        for (i, v) in items.iter().enumerate() {
                            if n == "arr.reduce" {
                                acc = Some(match acc {
                                    None => v.clone(),
                                    Some(p) => self.call(&f, alloc::vec![p, v.clone(), Value::Num(i as f64)], Value::Undef, doc)?,
                                });
                                continue;
                            }
                            let r = self.call(&f, alloc::vec![v.clone(), Value::Num(i as f64), this.clone()], Value::Undef, doc)?;
                            match n {
                                "arr.map" => out.push(r),
                                "arr.filter" => {
                                    if r.truthy() {
                                        out.push(v.clone());
                                    }
                                }
                                "arr.find" if r.truthy() => return Ok(v.clone()),
                                "arr.findIndex" if r.truthy() => return Ok(Value::Num(i as f64)),
                                "arr.some" if r.truthy() => return Ok(Value::Bool(true)),
                                "arr.every" if !r.truthy() => return Ok(Value::Bool(false)),
                                _ => {}
                            }
                        }
                        match n {
                            "arr.map" | "arr.filter" => new_arr(out),
                            "arr.reduce" => acc.unwrap_or(Value::Undef),
                            "arr.findIndex" => Value::Num(-1.0),
                            "arr.some" => Value::Bool(false),
                            "arr.every" => Value::Bool(true),
                            _ => Value::Undef,
                        }
                    }
                }
            }
            _ => Value::Undef,
        })
    }
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

/// Joriy yadro stekida qolgan joy (bayt). Steklar 64 KiB oynalarda, pastki 4 KiB — qörğon.
#[cfg(target_arch = "aarch64")]
fn stack_left() -> usize {
    let sp: usize;
    unsafe { core::arch::asm!("mov {}, sp", out(reg) sp) };
    (sp & 0xffff).saturating_sub(0x1000)
}

#[cfg(not(target_arch = "aarch64"))]
fn stack_left() -> usize {
    usize::MAX
}

/// Rekursiya çuqurligi sinovi: bitta JS çaqiruvi qança stek oladi (bayt).
pub fn stack_per_call() -> usize {
    let before = stack_left();
    let mut e = Engine::new();
    let mut doc = Doc::empty();
    let _ = e.run("function d(n) { if (n > 0) { d(n - 1); } else { __m = __sl(); } } d(8);", &mut doc, 0);
    let after = e.lookup(&e.globals.clone(), "__m").map(|v| v.num() as usize).unwrap_or(before);
    before.saturating_sub(after) / 8
}

fn gfx_w() -> usize {
    crate::gfx::size().0
}
fn gfx_h() -> usize {
    crate::gfx::size().1
}

/// localStorage: (manba, kalit, qiymat). `~/.zamin/localstorage` ga saqlanadi —
/// qayta yuklanişdan keyin ham qoladi.
static LS: crate::sync::SpinLock<Vec<(String, String, String)>> = crate::sync::SpinLock::new(Vec::new());
static LS_DISK: crate::net::webstore::Persist = crate::net::webstore::Persist::new("localstorage");

/// Omborga kiriş (joriy foydalanuvçining fayli bilan moslangan holda).
fn ls() -> crate::sync::SpinLockGuard<'static, Vec<(String, String, String)>> {
    LS_DISK.sync(&LS);
    LS.lock()
}

fn ls_save() {
    LS_DISK.save(&LS);
}

/// Manba (origin): "sxema://host" yoki ichki/mahalliy sahifalar uçun "zamin".
fn origin(url: &str) -> String {
    match url.split_once("://") {
        Some((sc, rest)) if sc != "zamin" => format!("{}://{}", sc, rest.split('/').next().unwrap_or("")),
        _ => String::from("zamin"),
    }
}

fn ls_set(url: &str, k: &str, v: &str) {
    let o = origin(url);
    {
        let mut ls = ls();
        match ls.iter().position(|(oo, kk, _)| *oo == o && kk == k) {
            Some(i) if ls[i].2 == v => return,
            Some(i) => ls[i].2 = String::from(v),
            None if ls.len() < 2000 => ls.push((o, String::from(k), String::from(v))),
            None => return,
        }
    }
    ls_save();
}

/// URL kodlaş (UTF-8 baytlar %XX).
pub fn url_encode(s: &str, keep_reserved: bool) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        let c = b as char;
        if c.is_ascii_alphanumeric() || "-_.!~*'()".contains(c) || (keep_reserved && ";,/?:@&=+$#".contains(c)) {
            out.push(c);
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
}

/// URL dan yeçiş (%XX va + → bo'şliq).
pub fn url_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(core::str::from_utf8(&b[i + 1..(i + 3).min(b.len())]).unwrap_or("zz"), 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(if b[i] == b'+' { b' ' } else { b[i] });
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// JSON tahlilçisi → JS qiymatlari.
pub fn json_parse(src: &str) -> Option<Value> {
    fn ws(b: &[u8], i: &mut usize) {
        while *i < b.len() && b[*i].is_ascii_whitespace() {
            *i += 1;
        }
    }
    fn string(b: &[u8], i: &mut usize) -> Option<String> {
        if b.get(*i) != Some(&b'"') {
            return None;
        }
        *i += 1;
        let mut out: Vec<u8> = Vec::new();
        while *i < b.len() && b[*i] != b'"' {
            if b[*i] == b'\\' && *i + 1 < b.len() {
                *i += 1;
                match b[*i] {
                    b'n' => out.push(b'\n'),
                    b't' => out.push(b'\t'),
                    b'r' => out.push(b'\r'),
                    b'b' | b'f' => {}
                    b'u' if *i + 4 < b.len() => {
                        let h = core::str::from_utf8(&b[*i + 1..*i + 5]).ok()?;
                        let c = char::from_u32(u32::from_str_radix(h, 16).ok()?).unwrap_or('?');
                        let mut buf = [0u8; 4];
                        out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
                        *i += 4;
                    }
                    x => out.push(x),
                }
                *i += 1;
                continue;
            }
            out.push(b[*i]);
            *i += 1;
        }
        *i += 1;
        Some(String::from_utf8_lossy(&out).into_owned())
    }
    fn val(b: &[u8], i: &mut usize, depth: usize) -> Option<Value> {
        if depth > 64 {
            return None;
        }
        ws(b, i);
        match *b.get(*i)? {
            b'{' => {
                *i += 1;
                let mut o = Vec::new();
                ws(b, i);
                if b.get(*i) == Some(&b'}') {
                    *i += 1;
                    return Some(Value::Obj(Rc::new(RefCell::new(o))));
                }
                loop {
                    ws(b, i);
                    let k = string(b, i)?;
                    ws(b, i);
                    if b.get(*i) != Some(&b':') {
                        return None;
                    }
                    *i += 1;
                    let v = val(b, i, depth + 1)?;
                    o.push((k, v));
                    ws(b, i);
                    match b.get(*i)? {
                        b',' => *i += 1,
                        b'}' => {
                            *i += 1;
                            return Some(Value::Obj(Rc::new(RefCell::new(o))));
                        }
                        _ => return None,
                    }
                }
            }
            b'[' => {
                *i += 1;
                let mut a = Vec::new();
                ws(b, i);
                if b.get(*i) == Some(&b']') {
                    *i += 1;
                    return Some(Value::Arr(Rc::new(RefCell::new(a))));
                }
                loop {
                    a.push(val(b, i, depth + 1)?);
                    ws(b, i);
                    match b.get(*i)? {
                        b',' => *i += 1,
                        b']' => {
                            *i += 1;
                            return Some(Value::Arr(Rc::new(RefCell::new(a))));
                        }
                        _ => return None,
                    }
                }
            }
            b'"' => string(b, i).map(|s| Value::str(&s)),
            b't' if b[*i..].starts_with(b"true") => {
                *i += 4;
                Some(Value::Bool(true))
            }
            b'f' if b[*i..].starts_with(b"false") => {
                *i += 5;
                Some(Value::Bool(false))
            }
            b'n' if b[*i..].starts_with(b"null") => {
                *i += 4;
                Some(Value::Null)
            }
            _ => {
                let st = *i;
                while *i < b.len() && (b[*i].is_ascii_digit() || b"+-.eE".contains(&b[*i])) {
                    *i += 1;
                }
                let t = core::str::from_utf8(&b[st..*i]).ok()?;
                if t.is_empty() {
                    return None;
                }
                let toks = lex(t).ok()?;
                match toks.as_slice() {
                    [Tok::Num(n), Tok::Eof] => Some(Value::Num(*n)),
                    [Tok::P("-"), Tok::Num(n), Tok::Eof] => Some(Value::Num(-*n)),
                    _ => None,
                }
            }
        }
    }
    let b = src.as_bytes();
    let mut i = 0;
    let v = val(b, &mut i, 0)?;
    ws(b, &mut i);
    if i == b.len() { Some(v) } else { None }
}


fn slice_range(len: usize, a: &Value, b: &Value) -> (usize, usize) {
    let norm = |v: &Value, d: usize| -> usize {
        if matches!(v, Value::Undef) {
            return d;
        }
        let n = v.num();
        let n = if n.is_nan() { 0.0 } else { n };
        if n < 0.0 { (len as f64 + n).max(0.0) as usize } else { (n as usize).min(len) }
    };
    let s = norm(a, 0);
    let e = norm(b, len);
    (s, e.max(s))
}

fn strict_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Num(x), Value::Num(y)) => x == y,
        (Value::Str(x), Value::Str(y)) => x == y,
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::Undef, Value::Undef) | (Value::Null, Value::Null) => true,
        (Value::Arr(x), Value::Arr(y)) => Rc::ptr_eq(x, y),
        (Value::Obj(x), Value::Obj(y)) => Rc::ptr_eq(x, y),
        (Value::Elem(x), Value::Elem(y)) => x == y,
        (Value::Host(x), Value::Host(y)) => x == y,
        _ => false,
    }
}

fn loose_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Undef | Value::Null, Value::Undef | Value::Null) => true,
        (Value::Undef | Value::Null, _) | (_, Value::Undef | Value::Null) => false,
        (Value::Str(_), Value::Num(_)) | (Value::Num(_), Value::Str(_)) | (Value::Bool(_), _) | (_, Value::Bool(_)) => a.num() == b.num(),
        _ => strict_eq(a, b),
    }
}

fn binop(op: &str, l: &Value, r: &Value) -> Value {
    match op {
        "+" => {
            if matches!(l, Value::Str(_) | Value::Arr(_) | Value::Obj(_) | Value::Date(_)) || matches!(r, Value::Str(_) | Value::Arr(_) | Value::Obj(_) | Value::Date(_)) {
                let mut s = l.to_string();
                s.push_str(&r.to_string());
                Value::str(&s)
            } else {
                Value::Num(l.num() + r.num())
            }
        }
        "-" => Value::Num(l.num() - r.num()),
        "*" => Value::Num(l.num() * r.num()),
        "/" => Value::Num(l.num() / r.num()),
        "%" => {
            let (a, b) = (l.num(), r.num());
            Value::Num(if b == 0.0 { f64::NAN } else { a - b * ((a / b) as i64) as f64 })
        }
        "**" => Value::Num(math_pow(l.num(), r.num())),
        "==" => Value::Bool(loose_eq(l, r)),
        "!=" => Value::Bool(!loose_eq(l, r)),
        "===" => Value::Bool(strict_eq(l, r)),
        "!==" => Value::Bool(!strict_eq(l, r)),
        "<" | ">" | "<=" | ">=" => {
            let res = if let (Value::Str(a), Value::Str(b)) = (l, r) {
                match op {
                    "<" => a < b,
                    ">" => a > b,
                    "<=" => a <= b,
                    _ => a >= b,
                }
            } else {
                let (a, b) = (l.num(), r.num());
                match op {
                    "<" => a < b,
                    ">" => a > b,
                    "<=" => a <= b,
                    _ => a >= b,
                }
            };
            Value::Bool(res)
        }
        "&" => Value::Num(((l.num() as i64 as i32) & (r.num() as i64 as i32)) as f64),
        "|" => Value::Num(((l.num() as i64 as i32) | (r.num() as i64 as i32)) as f64),
        "^" => Value::Num(((l.num() as i64 as i32) ^ (r.num() as i64 as i32)) as f64),
        "in" => Value::Bool(match r {
            Value::Obj(o) => o.borrow().iter().any(|(k, _)| *k == l.to_string()),
            Value::Arr(a) => (l.num() as usize) < a.borrow().len(),
            _ => false,
        }),
        "instanceof" => Value::Bool(false),
        _ => Value::Undef,
    }
}

fn math_pow(b: f64, e: f64) -> f64 {
    if e == (e as i32) as f64 {
        return powi(b, e as i32);
    }
    exp(e * ln(b))
}

/// Natural logarifm (qatorga yoyiş: 2·atanh).
fn ln(x: f64) -> f64 {
    if x.is_nan() || x < 0.0 {
        return f64::NAN;
    }
    if x == 0.0 {
        return f64::NEG_INFINITY;
    }
    let (mut m, mut k) = (x, 0i32);
    while m >= 2.0 {
        m /= 2.0;
        k += 1;
    }
    while m < 1.0 {
        m *= 2.0;
        k -= 1;
    }
    let z = (m - 1.0) / (m + 1.0);
    let (mut term, mut sum) = (z, 0.0);
    let z2 = z * z;
    let mut n = 1.0;
    while n < 60.0 {
        sum += term / n;
        term *= z2;
        n += 2.0;
    }
    2.0 * sum + k as f64 * core::f64::consts::LN_2
}

fn exp(x: f64) -> f64 {
    if x.is_nan() {
        return x;
    }
    if x > 700.0 {
        return f64::INFINITY;
    }
    if x < -700.0 {
        return 0.0;
    }
    let k = (x / core::f64::consts::LN_2) as i32;
    let r = x - k as f64 * core::f64::consts::LN_2;
    let (mut term, mut sum) = (1.0, 1.0);
    for i in 1..30 {
        term *= r / i as f64;
        sum += term;
    }
    sum * powi(2.0, k)
}

fn math_name(k: &str) -> &'static str {
    match k {
        "floor" => "Math.floor",
        "ceil" => "Math.ceil",
        "round" => "Math.round",
        "abs" => "Math.abs",
        "max" => "Math.max",
        "min" => "Math.min",
        "random" => "Math.random",
        "sqrt" => "Math.sqrt",
        "pow" => "Math.pow",
        "sin" => "Math.sin",
        "cos" => "Math.cos",
        "trunc" => "Math.trunc",
        "sign" => "Math.sign",
        _ => "noop",
    }
}

fn math(n: &str, a: &[Value]) -> Value {
    let x = a.first().map(|v| v.num()).unwrap_or(f64::NAN);
    let floor = |v: f64| {
        let t = v as i64 as f64;
        if t > v { t - 1.0 } else { t }
    };
    Value::Num(match n {
        "Math.floor" => floor(x),
        "Math.ceil" => -floor(-x),
        "Math.round" => floor(x + 0.5),
        "Math.trunc" => x as i64 as f64,
        "Math.abs" => x.abs(),
        "Math.sign" => if x > 0.0 { 1.0 } else if x < 0.0 { -1.0 } else { 0.0 },
        "Math.max" => a.iter().map(|v| v.num()).fold(f64::NEG_INFINITY, |m, v| if v > m { v } else { m }),
        "Math.min" => a.iter().map(|v| v.num()).fold(f64::INFINITY, |m, v| if v < m { v } else { m }),
        "Math.random" => (crate::random::u64() >> 11) as f64 / (1u64 << 53) as f64,
        "Math.sqrt" => crate::image::vector::fsqrt(x as f32) as f64,
        "Math.pow" => math_pow(x, a.get(1).map(|v| v.num()).unwrap_or(f64::NAN)),
        "Math.sin" => crate::image::vector::sin(x as f32) as f64,
        "Math.cos" => crate::image::vector::cos(x as f32) as f64,
        _ => f64::NAN,
    })
}

fn str_method(k: &str) -> &'static str {
    match k {
        "toUpperCase" => "str.upper",
        "toLowerCase" => "str.lower",
        "indexOf" => "str.indexOf",
        "lastIndexOf" => "str.lastIndexOf",
        "includes" => "str.includes",
        "startsWith" => "str.startsWith",
        "endsWith" => "str.endsWith",
        "slice" => "str.slice",
        "substring" => "str.substring",
        "substr" => "str.substr",
        "split" => "str.split",
        "trim" => "str.trim",
        "trimStart" => "str.trimStart",
        "trimEnd" => "str.trimEnd",
        "replace" => "str.replace",
        "replaceAll" => "str.replaceAll",
        "repeat" => "str.repeat",
        "charAt" => "str.charAt",
        "charCodeAt" => "str.charCodeAt",
        "padStart" => "str.padStart",
        "padEnd" => "str.padEnd",
        "concat" => "str.concat",
        _ => "any.toString",
    }
}

fn str_call(n: &str, this: &Value, a: &[Value]) -> Value {
    let s = this.to_string();
    let chars: Vec<char> = s.chars().collect();
    let sa = |k: usize| a.get(k).map(|v| v.to_string()).unwrap_or_default();
    let na = |k: usize| a.get(k).map(|v| v.num()).unwrap_or(f64::NAN);
    let cidx = |byte: usize| s[..byte].chars().count() as f64;
    match n {
        "str.upper" => Value::str(&s.to_uppercase()),
        "str.lower" => Value::str(&s.to_lowercase()),
        "str.indexOf" => Value::Num(s.find(&sa(0)).map(cidx).unwrap_or(-1.0)),
        "str.lastIndexOf" => Value::Num(s.rfind(&sa(0)).map(cidx).unwrap_or(-1.0)),
        "str.includes" => Value::Bool(s.contains(&sa(0))),
        "str.startsWith" => Value::Bool(s.starts_with(&sa(0))),
        "str.endsWith" => Value::Bool(s.ends_with(&sa(0))),
        "str.slice" | "str.substring" => {
            let (mut st, mut en) = slice_range(chars.len(), a.first().unwrap_or(&Value::Undef), a.get(1).unwrap_or(&Value::Undef));
            if n == "str.substring" && st > en {
                core::mem::swap(&mut st, &mut en);
            }
            Value::str(&chars[st..en.max(st)].iter().collect::<String>())
        }
        "str.substr" => {
            let (st, _) = slice_range(chars.len(), a.first().unwrap_or(&Value::Undef), &Value::Undef);
            let len = if a.len() > 1 { (na(1).max(0.0) as usize).min(chars.len() - st) } else { chars.len() - st };
            Value::str(&chars[st..st + len].iter().collect::<String>())
        }
        "str.split" => {
            let parts: Vec<Value> = if a.is_empty() {
                alloc::vec![Value::str(&s)]
            } else if sa(0).is_empty() {
                chars.iter().map(|c| Value::str(&c.to_string())).collect()
            } else {
                s.split(sa(0).as_str()).map(Value::str).collect()
            };
            Value::Arr(Rc::new(RefCell::new(parts)))
        }
        "str.trim" => Value::str(s.trim()),
        "str.trimStart" => Value::str(s.trim_start()),
        "str.trimEnd" => Value::str(s.trim_end()),
        "str.replace" => Value::str(&s.replacen(sa(0).as_str(), &sa(1), 1)),
        "str.replaceAll" => Value::str(&s.replace(sa(0).as_str(), &sa(1))),
        "str.repeat" => Value::str(&s.repeat(na(0).clamp(0.0, 10000.0) as usize)),
        "str.charAt" => Value::str(&chars.get(na(0).max(0.0) as usize).map(|c| c.to_string()).unwrap_or_default()),
        "str.charCodeAt" => Value::Num(chars.get(na(0).max(0.0) as usize).map(|c| *c as u32 as f64).unwrap_or(f64::NAN)),
        "str.padStart" | "str.padEnd" => {
            let w = na(0).max(0.0) as usize;
            let pad = if a.len() > 1 { sa(1) } else { String::from(" ") };
            let mut fill = String::new();
            while chars.len() + fill.chars().count() < w && !pad.is_empty() {
                fill.push_str(&pad);
            }
            let fill: String = fill.chars().take(w.saturating_sub(chars.len())).collect();
            Value::str(&if n == "str.padStart" { fill + &s } else { s + &fill })
        }
        "str.concat" => Value::str(&(s + &a.iter().map(|v| v.to_string()).collect::<String>())),
        _ => Value::str(&s),
    }
}

fn date_method(k: &str) -> &'static str {
    match k {
        "getFullYear" => "date.year",
        "getMonth" => "date.month",
        "getDate" => "date.day",
        "getDay" => "date.wday",
        "getHours" => "date.hour",
        "getMinutes" => "date.min",
        "getSeconds" => "date.sec",
        "getMilliseconds" => "date.ms",
        "getTime" => "date.time",
        "toLocaleTimeString" => "date.ltime",
        "toLocaleDateString" => "date.ldate",
        _ => "date.str",
    }
}

fn date_call(n: &str, this: &Value) -> Value {
    let ms = this.num();
    let t = crate::time::local((ms / 1000.0) as u64);
    match n {
        "date.year" => Value::Num(t.year as f64),
        "date.month" => Value::Num(t.month as f64 - 1.0),
        "date.day" => Value::Num(t.day as f64),
        "date.wday" => {
            // 1970-01-01 — payşanba (4)
            let days = (ms / 86_400_000.0) as i64;
            Value::Num(((days + 4) % 7) as f64)
        }
        "date.hour" => Value::Num(t.hour as f64),
        "date.min" => Value::Num(t.minute as f64),
        "date.sec" => Value::Num(t.second as f64),
        "date.ms" => Value::Num((ms as u64 % 1000) as f64),
        "date.time" => Value::Num(ms),
        "date.ltime" => Value::str(&date_str(ms, 2)),
        "date.ldate" => Value::str(&date_str(ms, 1)),
        _ => Value::str(&date_str(ms, 0)),
    }
}

/// O'z-o'zini sinaş (yuklanişda): asosiy til imkoniyatlari.
fn cv_method(k: &str) -> &'static str {
    match k {
        "fillRect" => "cv.fillRect",
        "strokeRect" => "cv.strokeRect",
        "clearRect" => "cv.clearRect",
        "fillText" => "cv.fillText",
        "strokeText" => "cv.strokeText",
        "measureText" => "cv.measureText",
        "beginPath" => "cv.beginPath",
        "moveTo" => "cv.moveTo",
        "lineTo" => "cv.lineTo",
        "closePath" => "cv.closePath",
        "rect" => "cv.rect",
        "arc" => "cv.arc",
        "fill" => "cv.fill",
        "stroke" => "cv.stroke",
        _ => "noop", // save/restore/setTransform … — hozirça e'tiborsiz
    }
}

/// CanvasRenderingContext2D usullari. `this` — kontekst obyekti (holat: fillStyle, …).
fn canvas_call(name: &str, this: &Value, a: &[Value], doc: &mut Doc) -> Value {
    let Value::Obj(o) = this else { return Value::Undef };
    let prop = |k: &str| o.borrow().iter().find(|(kk, _)| kk == k).map(|(_, v)| v.clone()).unwrap_or(Value::Undef);
    let n = prop("__cv").num();
    let Some(cv) = (n >= 0.0).then(|| n as usize).and_then(|n| doc.canvases.get_mut(n)) else { return Value::Undef };
    let f = |k: usize| a.get(k).map(|v| v.num() as f32).unwrap_or(f32::NAN);
    let ga = {
        let g = prop("globalAlpha").num();
        if g.is_finite() { g.clamp(0.0, 1.0) as f32 } else { 1.0 }
    };
    let style = |k: &str| -> Option<(u32, u32)> {
        crate::canvas::parse_style(&prop(k).to_string()).map(|(c, al)| (c, (al as f32 * ga + 0.5) as u32))
    };
    let lw = {
        let w = prop("lineWidth").num() as f32;
        if w > 0.0 && w.is_finite() { w.min(500.0) } else { 1.0 }
    };
    match name {
        "cv.fillRect" => {
            if let Some((c, al)) = style("fillStyle") {
                cv.fill_rect(f(0), f(1), f(2), f(3), c, al);
            }
        }
        "cv.strokeRect" => {
            if let Some((c, al)) = style("strokeStyle") {
                cv.stroke_rect(f(0), f(1), f(2), f(3), lw, c, al);
            }
        }
        "cv.clearRect" => cv.clear_rect(f(0), f(1), f(2), f(3)),
        "cv.fillText" | "cv.strokeText" => {
            let k = if name == "cv.fillText" { "fillStyle" } else { "strokeStyle" };
            if let Some((c, al)) = style(k) {
                let txt = a.first().map(|v| v.to_string()).unwrap_or_default();
                let align = match prop("textAlign").to_string().as_str() {
                    "center" => 1,
                    "right" | "end" => 2,
                    _ => 0,
                };
                let base = match prop("textBaseline").to_string().as_str() {
                    "top" | "hanging" => 1,
                    "middle" => 2,
                    "bottom" | "ideographic" => 3,
                    _ => 0,
                };
                cv.fill_text(&txt, f(1), f(2), crate::canvas::font_px(&prop("font").to_string()), align, base, c, al);
            }
        }
        "cv.measureText" => {
            let txt = a.first().map(|v| v.to_string()).unwrap_or_default();
            let px = crate::canvas::font_px(&prop("font").to_string()).clamp(4, 200);
            return Value::Obj(Rc::new(RefCell::new(alloc::vec![(String::from("width"), Value::Num(crate::gfx::text_width(&txt, px) as f64))])));
        }
        "cv.beginPath" => cv.begin_path(),
        "cv.moveTo" => cv.move_to(f(0), f(1)),
        "cv.lineTo" => cv.line_to(f(0), f(1)),
        "cv.closePath" => cv.close_path(),
        "cv.rect" => cv.rect(f(0), f(1), f(2), f(3)),
        "cv.arc" => cv.arc(f(0), f(1), f(2), f(3), f(4), a.get(5).is_some_and(|v| v.truthy())),
        "cv.fill" => {
            if let Some((c, al)) = style("fillStyle") {
                cv.fill_path(c, al);
            }
        }
        "cv.stroke" => {
            if let Some((c, al)) = style("strokeStyle") {
                cv.stroke_path(lw, c, al);
            }
        }
        _ => {}
    }
    Value::Undef
}

/// <canvas> 2D: JS dan çizib, buferdagi piksellarni tekşiradi.
pub fn self_test_canvas() -> Result<usize, String> {
    let mut doc = crate::html::parse("<canvas id=c width=40 height=30></canvas>");
    let mut e = Engine::new();
    e.run(
        "const c = document.getElementById('c'); const x = c.getContext('2d');\
         x.fillStyle = '#ff0000'; x.fillRect(0, 0, 10, 10);\
         x.strokeStyle = 'rgb(0, 0, 255)'; x.lineWidth = 2; x.strokeRect(20, 2, 10, 10);\
         x.beginPath(); x.moveTo(0, 20); x.lineTo(39, 20); x.strokeStyle = '#00ff00'; x.stroke();\
         x.fillStyle = 'rgba(0,0,0,0.5)'; x.beginPath(); x.arc(30, 25, 4, 0, Math.PI * 2); x.fill();\
         x.fillStyle = '#000'; x.font = '12px sans-serif'; x.fillText('Hi', 2, 28);\
         window.__w = c.width + x.measureText('Hi').width;",
        &mut doc,
        0,
    )?;
    let cv = doc.canvases.first().ok_or("canvas buferi yaratilmadi")?;
    let at = |x: usize, y: usize| cv.px[y * cv.w + x];
    let mut n = 0;
    let mut chk = |ok: bool, what: &str| -> Result<(), String> {
        if !ok {
            return Err(format!("canvas: {}", what));
        }
        n += 1;
        Ok(())
    };
    chk(cv.w == 40 && cv.h == 30, "ölçam")?;
    chk(at(5, 5) == 0xffff0000, "fillRect")?;
    chk(at(15, 5) == 0, "şaffof fon")?;
    chk(at(20, 7) & 0xffffff == 0x0000ff && at(25, 7) == 0, "strokeRect")?;
    chk(at(10, 20) & 0x00ff00 == 0x00ff00 && at(10, 20) >> 24 > 100, "lineTo/stroke")?;
    let m = at(30, 25) >> 24;
    chk((110..=145).contains(&m), "arc/fill (rgba alfa)")?;
    chk((0..cv.w * cv.h).any(|k| k / cv.w >= 18 && k % cv.w < 16 && cv.px[k] >> 24 > 200 && cv.px[k] & 0xffffff == 0), "fillText")?;
    Ok(n)
}

pub fn self_test() -> Result<usize, String> {
    let cases: [(&str, &str); 19] = [
        ("async function f(x) { return x * 2; } let r = 0; f(21).then(v => { r = v; }); r", "0"),
        ("async function g() { const a = await Promise.resolve(5); const b = await new Promise(r => r(7)); return a + b; } let z = 0; g().then(v => { z = v; }); z", "0"),
        ("JSON.parse('{\"a\": [1, 2, {\"b\": \"x\"}], \"n\": -3.5}').a[2].b + JSON.parse('[1,2]').length", "x2"),
        ("JSON.stringify(JSON.parse('{\"k\":[true,null,\"s\"]}'))", "{\"k\":[true,null,\"s\"]}"),
        ("decodeURIComponent(encodeURIComponent('Salom dunyo & ş'))", "Salom dunyo & ş"),
        ("Math.max(...[3, 9, 4]) + String(1).padStart(3, '0')", "9001"),
        ("const p = x => String(x).padStart(2, '0'); p(7) + ':' + p(12)", "07:12"),
        ("1 + 2 * 3", "7"),
        ("'a' + 1 + 2", "a12"),
        ("let s = 0; for (let i = 1; i <= 10; i++) s += i; s", "55"),
        ("function f(n) { return n < 2 ? n : f(n - 1) + f(n - 2); } f(15)", "610"),
        ("[1,2,3].map(x => x * x).join('-')", "1-4-9"),
        ("let o = {a: 1, b: 'x'}; o.c = 3; Object.keys(o).length", "3"),
        ("`${2 + 3} ta`", "5 ta"),
        ("let a = []; a.push(3, 1, 2); a.sort((x, y) => x - y); a.join()", "1,2,3"),
        ("function mk() { let c = 0; return () => ++c; } let g = mk(); g(); g(); g()", "3"),
        ("'Salom Dunyo'.toUpperCase().split(' ')[1]", "DUNYO"),
        ("try { throw 'xato'; } catch (e) { 'ushlandi: ' + e }", "ushlandi: xato"),
        ("[1, 2, 3, 4].filter(x => x % 2 == 0).reduce((s, x) => s + x, 0)", "6"),
    ];
    let mut n = 0;
    for (code, want) in cases {
        let mut e = Engine::new();
        let mut doc = Doc::empty();
        // oxirgi ifoda qiymatini oliş uçun: __r = (…)
        let prog = lex(code).and_then(|t| Parser { t, i: 0 }.program())?;
        let g = e.globals.clone();
        let mut last = Value::Undef;
        for f in &prog {
            if let Stmt::Fun(nm, d) = f {
                g.borrow_mut().vars.push((nm.clone(), Value::Func(d.clone(), g.clone())));
            }
        }
        for s in &prog {
            match s {
                Stmt::Expr(x) => last = e.eval(x, &g, &mut doc).map_err(|_| format!("bajarilmadi: {}", code))?,
                Stmt::Try(..) => {
                    // try ichidagi oxirgi ifoda: catch blokidagini olamiz
                    if let Stmt::Try(_, Some((nm, cb)), _) = s {
                        let env = new_env(Some(g.clone()));
                        env.borrow_mut().vars.push((nm.clone(), Value::str("xato")));
                        if let Some(Stmt::Expr(x)) = cb.last() {
                            last = e.eval(x, &env, &mut doc).map_err(|_| format!("bajarilmadi: {}", code))?;
                        }
                    }
                }
                s => {
                    e.exec(s, &g, &mut doc).map_err(|_| format!("bajarilmadi: {}", code))?;
                }
            }
        }
        let got = last.to_string();
        if got != want {
            return Err(format!("{} → {} (kutilgan {})", code, got, want));
        }
        n += 1;
    }
    Ok(n)
}
