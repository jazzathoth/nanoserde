#[path = "json/helpers.rs"]
mod helpers;

use ::core::str::Chars;
use ::core::{error::Error, fmt::Write, time::Duration};

use ::alloc::boxed::Box;
use ::alloc::collections::{BTreeMap, BTreeSet, LinkedList};
use ::alloc::format;
use ::alloc::string::{String, ToString};
use ::alloc::vec::Vec;

#[cfg(feature = "std")]
use ::alloc::sync::Arc;

/// The internal state of a JSON serialization.
#[non_exhaustive]
pub struct SerJsonState {
    pub out: String,
}

impl SerJsonState {
    pub fn new(out: String) -> Self {
        Self { out }
    }

    pub fn indent(&mut self, _d: usize) {
        //for _ in 0..d {
        //    self.out.push_str("    ");
        //}
    }

    pub fn field(&mut self, d: usize, field: &str) {
        self.indent(d);
        self.out.push('"');
        self.out.push_str(field);
        self.out.push('"');
        self.out.push(':');
    }

    pub fn label(&mut self, label: &str) {
        self.out.push('"');
        self.out.push_str(label);
        self.out.push('"');
    }

    pub fn conl(&mut self) {
        self.out.push(',')
    }

    pub fn st_pre(&mut self) {
        self.out.push('{');
    }

    pub fn st_post(&mut self, d: usize) {
        self.indent(d);
        self.out.push('}');
    }
}

/// A trait for objects that can be serialized to JSON.
pub trait SerJson {
    /// Serialize Self to a JSON string.
    ///
    /// This is a convenient wrapper around `ser_json`.
    fn serialize_json(&self) -> String {
        let mut s = SerJsonState { out: String::new() };
        self.ser_json(0, &mut s);
        s.out
    }

    /// Serialize Self to a JSON string.
    ///
    /// ```rust
    /// # use nanoserde::*;
    /// let mut s = SerJsonState::new(String::new());
    /// 42u32.ser_json(0, &mut s);
    /// assert_eq!(s.out, "42");
    /// ```
    fn ser_json(&self, d: usize, s: &mut SerJsonState);
}

/// A trait for objects that can be deserialized from JSON.
pub trait DeJson: Sized {
    /// Deserialize exactly one JSON value, allowing surrounding whitespace.
    fn deserialize_json(input: &str) -> Result<Self, DeJsonErr> {
        let mut state = DeJsonState::new(input);
        let mut root = DeJsonScope::root(&mut state);
        root.next_tok()?;
        if root.done {
            return Err(root.err_token("JSON value"));
        }
        let value = Self::de_json(&mut root)?;
        if matches!(root.token(), DeJsonTok::CurlyClose | DeJsonTok::BlockClose) {
            root.next_tok()?;
        }
        if !root.done || *root.token() != DeJsonTok::Eof {
            return Err(root.err_token("end of input"));
        }
        Ok(value)
    }

    /// Read one value. Container values receive their own temporary scope.
    fn de_json(parent: &mut DeJsonScope<'_, '_>) -> Result<Self, DeJsonErr> {
        if matches!(parent.token(), DeJsonTok::CurlyOpen | DeJsonTok::BlockOpen) {
            let mut child = parent.new_scope()?;
            let value = Self::de_json_value(&mut child)?;
            child.finish()?;
            Ok(value)
        } else {
            Self::de_json_value(parent)
        }
    }

    /// Implement reading within the scope selected by `de_json`.
    fn de_json_value(scope: &mut DeJsonScope<'_, '_>) -> Result<Self, DeJsonErr>;
}

/// A JSON parsed token.
#[derive(PartialEq, Debug, Default, Clone)]
#[non_exhaustive]
pub enum DeJsonTok {
    Str,
    Char(char),
    U64(u64),
    I64(i64),
    F64(f64),
    Bool(bool),
    BareIdent,
    Null,
    Colon,
    CurlyOpen,
    CurlyClose,
    BlockOpen,
    BlockClose,
    Comma,
    #[default]
    Bof,
    Eof,
}

/// The internal state of a JSON deserialization.
#[non_exhaustive]
pub struct DeJsonState<'input> {
    input: Chars<'input>,
    cur: char,
    tok: DeJsonTok,
    strbuf: String,
    numbuf: String,
    identbuf: String,
    line: usize,
    col: usize,
}

#[derive(Clone)]
#[non_exhaustive]
pub enum DeJsonErrReason {
    Separator,
    UnexpectedKey(String),
    UnexpectedToken(DeJsonTok, String),
    MissingKey(String),
    NoSuchEnum(String),
    OutOfRange(String),
    WrongType(String),
    CannotParse(String),
    #[cfg(feature = "std")]
    UserError(Arc<dyn Error + Send + Sync + 'static>),
}

/// The error message when failing to deserialize a JSON string.
#[derive(Clone)]
pub struct DeJsonErr {
    line: usize,
    col: usize,
    pub msg: DeJsonErrReason,
}

impl core::fmt::Debug for DeJsonErrReason {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Separator => f.write_str("Expected separator or closing delimiter"),
            Self::UnexpectedKey(name) => write!(f, "Unexpected key {}", name),
            Self::MissingKey(name) => write!(f, "Key not found {}", name),
            Self::NoSuchEnum(name) => write!(f, "Enum not defined {}", name),
            Self::UnexpectedToken(token, name) => {
                write!(f, "Unexpected token {:?} expected {} ", token, name)
            }
            Self::OutOfRange(value) => write!(f, "Value out of range {} ", value),
            Self::WrongType(found) => write!(f, "Token wrong type {} ", found),
            Self::CannotParse(unparseable) => write!(f, "Cannot parse {} ", unparseable),
            #[cfg(feature = "std")]
            Self::UserError(error) => f.debug_tuple("UserError").field(error).finish(),
        }
    }
}

impl core::fmt::Debug for DeJsonErr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let DeJsonErr {
            line,
            col: column,
            msg: reason,
        } = self;
        write!(
            f,
            "Json Deserialize error: {:?}, line:{} col:{}",
            reason,
            line + 1,
            column + 1
        )
    }
}

impl core::fmt::Display for DeJsonErr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Debug::fmt(self, f)
    }
}

impl Error for DeJsonErr {}

impl Default for DeJsonState<'_> {
    fn default() -> Self {
        Self {
            input: "".chars(),
            cur: '\0',
            tok: DeJsonTok::Bof,
            strbuf: String::new(),
            numbuf: String::new(),
            identbuf: String::new(),
            line: 0,
            col: 0,
        }
    }
}

impl<'input> DeJsonState<'input> {
    pub fn new(input: &'input str) -> Self {
        let mut state = Self {
            input: input.chars(),
            ..Self::default()
        };
        state.next();
        state
    }

    fn next(&mut self) {
        if let Some(c) = self.input.next() {
            self.cur = c;
            if self.cur == '\n' {
                self.line += 1;
                self.col = 0;
            } else {
                self.col += 1;
            }
        } else {
            self.cur = '\0';
        }
    }

    pub fn err_exp(&self, name: &str) -> DeJsonErr {
        DeJsonErr {
            msg: DeJsonErrReason::UnexpectedKey(name.to_string()),
            line: self.line,
            col: self.col,
        }
    }

    pub fn err_nf(&self, name: &str) -> DeJsonErr {
        DeJsonErr {
            msg: DeJsonErrReason::MissingKey(name.to_string()),
            line: self.line,
            col: self.col,
        }
    }

    pub fn err_enum(&self, name: &str) -> DeJsonErr {
        DeJsonErr {
            msg: DeJsonErrReason::NoSuchEnum(name.to_string()),
            line: self.line,
            col: self.col,
        }
    }

    pub fn err_sep(&self) -> DeJsonErr {
        DeJsonErr {
            line: self.line,
            col: self.col,
            msg: DeJsonErrReason::Separator,
        }
    }

    pub fn err_token(&self, what: &str) -> DeJsonErr {
        DeJsonErr {
            msg: DeJsonErrReason::UnexpectedToken(self.tok.clone(), what.to_string()),
            line: self.line,
            col: self.col,
        }
    }

    pub fn err_range(&self, what: &str) -> DeJsonErr {
        DeJsonErr {
            msg: DeJsonErrReason::OutOfRange(what.to_string()),
            line: self.line,
            col: self.col,
        }
    }

    pub fn err_type(&self, what: &str) -> DeJsonErr {
        DeJsonErr {
            msg: DeJsonErrReason::WrongType(what.to_string()),
            line: self.line,
            col: self.col,
        }
    }

    pub fn err_parse(&self, what: &str) -> DeJsonErr {
        DeJsonErr {
            msg: DeJsonErrReason::CannotParse(what.to_string()),
            line: self.line,
            col: self.col,
        }
    }

    #[cfg(feature = "std")]
    pub fn err_user<E>(&self, error: E) -> DeJsonErr
    where
        E: Error + Send + Sync + 'static,
    {
        DeJsonErr {
            line: self.line,
            col: self.col,
            msg: DeJsonErrReason::UserError(Arc::new(error)),
        }
    }

    fn scan_token(&mut self) -> Result<(), DeJsonErr> {
        self.strbuf.clear();
        while self.cur == '\n' || self.cur == '\r' || self.cur == '\t' || self.cur == ' ' {
            self.next();
        }
        if self.cur == '\0' {
            self.tok = DeJsonTok::Eof;
            return Ok(());
        }
        if self.cur == '/' {
            self.next();
            match self.cur {
                '/' => {
                    while self.cur != '\n' && self.cur != '\0' {
                        self.next();
                    }
                    return self.scan_token();
                }
                '*' => {
                    let mut last = self.cur;
                    loop {
                        self.next();
                        if self.cur == '\0' {
                            return Err(self.err_token("MultiLineCommentClose"));
                        }
                        if last == '*' && self.cur == '/' {
                            self.next();
                            break;
                        }
                        last = self.cur;
                    }
                    return self.scan_token();
                }
                _ => {
                    return Err(self.err_token("CommentOpen"));
                }
            }
        }
        match self.cur {
            ':' => {
                self.next();
                self.tok = DeJsonTok::Colon;
                Ok(())
            }
            ',' => {
                self.next();
                self.tok = DeJsonTok::Comma;
                Ok(())
            }
            '[' => {
                self.next();
                self.tok = DeJsonTok::BlockOpen;
                Ok(())
            }
            ']' => {
                self.next();
                self.tok = DeJsonTok::BlockClose;
                Ok(())
            }
            '{' => {
                self.next();
                self.tok = DeJsonTok::CurlyOpen;
                Ok(())
            }
            '}' => {
                self.next();
                self.tok = DeJsonTok::CurlyClose;
                Ok(())
            }
            '-' | '+' | '0'..='9' => {
                self.numbuf.clear();
                let is_neg = if self.cur == '-' || self.cur == '+' {
                    let sign = self.cur;
                    self.numbuf.push(self.cur);
                    self.next();
                    sign == '-'
                } else {
                    false
                };
                while self.cur >= '0' && self.cur <= '9' {
                    self.numbuf.push(self.cur);
                    self.next();
                }
                let mut is_float = false;
                if self.cur == '.' {
                    is_float = true;
                    self.numbuf.push(self.cur);
                    self.next();
                    while self.cur >= '0' && self.cur <= '9' {
                        self.numbuf.push(self.cur);
                        self.next();
                    }
                }
                if self.cur == 'e' || self.cur == 'E' {
                    is_float = true;
                    self.numbuf.push(self.cur);
                    self.next();
                    if self.cur == '-' {
                        self.numbuf.push(self.cur);
                        self.next();
                    }
                    while self.cur >= '0' && self.cur <= '9' {
                        self.numbuf.push(self.cur);
                        self.next();
                    }
                }
                if is_float {
                    if let Ok(num) = self.numbuf.parse() {
                        self.tok = DeJsonTok::F64(num);
                        Ok(())
                    } else {
                        Err(self.err_parse("number"))
                    }
                } else {
                    if is_neg {
                        if let Ok(num) = self.numbuf.parse() {
                            self.tok = DeJsonTok::I64(num);
                            return Ok(());
                        } else {
                            return Err(self.err_parse("number"));
                        }
                    }
                    if let Ok(num) = self.numbuf.parse() {
                        self.tok = DeJsonTok::U64(num);
                        Ok(())
                    } else {
                        Err(self.err_parse("number"))
                    }
                }
            }
            'a'..='z' | 'A'..='Z' | '_' => {
                self.identbuf.clear();
                while self.cur >= 'a' && self.cur <= 'z'
                    || self.cur >= 'A' && self.cur <= 'Z'
                    || self.cur == '_'
                {
                    self.identbuf.push(self.cur);
                    self.next();
                }
                if self.identbuf == "true" {
                    self.tok = DeJsonTok::Bool(true);
                    return Ok(());
                }
                if self.identbuf == "false" {
                    self.tok = DeJsonTok::Bool(false);
                    return Ok(());
                }
                if self.identbuf == "null" {
                    self.tok = DeJsonTok::Null;
                    return Ok(());
                }
                self.tok = DeJsonTok::BareIdent;
                Err(self.err_token(&format!(
                    "Got ##{}## needed true, false, null",
                    self.identbuf
                )))
            }
            '"' => {
                self.strbuf.clear();
                self.next();
                while self.cur != '"' {
                    if self.cur == '\\' {
                        self.next();
                        match self.cur {
                            'n' => self.strbuf.push('\n'),
                            'r' => self.strbuf.push('\r'),
                            't' => self.strbuf.push('\t'),
                            'b' => self.strbuf.push('\x08'),
                            'f' => self.strbuf.push('\x0c'),
                            '0' => self.strbuf.push('\0'),
                            '\0' => {
                                return Err(self.err_parse("string"));
                            }
                            'u' => {
                                if let Some(c) = self.hex_unescape_char() {
                                    self.strbuf.push(c);
                                    continue;
                                } else {
                                    return Err(self.err_parse("string"));
                                }
                            }
                            _ => self.strbuf.push(self.cur),
                        }
                        self.next();
                    } else {
                        if self.cur == '\0' {
                            return Err(self.err_parse("string"));
                        }
                        self.strbuf.push(self.cur);
                        self.next();
                    }
                }
                self.next();
                self.tok = DeJsonTok::Str;
                Ok(())
            }
            _ => Err(self.err_token("tokenizer")),
        }
    }

    /// Helper for reading `\uXXXX` escapes out of a string, properly handing
    /// surrogate pairs (by potentially unescaping a second `\uXXXX` sequence if
    /// it would complete a surrogate pair).
    ///
    /// On illegal escapes or unpaired surrogates returns None (and caller
    /// should emit an error).
    fn hex_unescape_char(&mut self) -> Option<char> {
        self.next();
        let a = xdigit4(self)?;
        if let Some(c) = core::char::from_u32(a as u32) {
            return Some(c);
        }
        // `a` isn't a valid scalar, but if it's leading surrogate, we look for
        // a trailing surrogate in a `\uXXXX` sequence immediately after.
        let a_is_lead = (0xd800..0xdc00).contains(&a);
        if a_is_lead && self.cur == '\\' {
            self.next();
            if self.cur == 'u' {
                self.next();
                let b = xdigit4(self)?;
                let b_is_trail = (0xdc00..0xe000).contains(&b);
                if b_is_trail {
                    // It's a valid pair! We have `[a, b]` where `a` is a leading
                    // surrogate and `b` is a trailing one.
                    let scalar = (((a as u32 - 0xd800) << 10) | (b as u32 - 0xdc00)) + 0x10000;
                    // All valid surrogate pairs decode to unicode scalar values
                    // (e.g. `char`), so this block should always return `Some`, the
                    // debug_assert exists just to ensure our testing is thorough
                    // enough.
                    let ch = core::char::from_u32(scalar);
                    debug_assert!(ch.is_some());
                    return ch;
                }
            }
        }
        return None;

        // Helper to turn next 4 ascii hex digits into a u16
        fn xdigit4(de: &mut DeJsonState<'_>) -> Option<u16> {
            // as tempting as it is to try to find a way to use from_str_radix on the
            // next 4 bytes from the character cursor, we'd still need to do validation to detect cases
            // like `\u+123` and such which makes it less attractive.
            (0..4).try_fold(0u16, |acc, _| {
                let n = match de.cur {
                    '0'..='9' => de.cur as u16 - '0' as u16,
                    'a'..='f' => de.cur as u16 - 'a' as u16 + 10,
                    'A'..='F' => de.cur as u16 - 'A' as u16 + 10,
                    _ => return None,
                };
                de.next();
                Some(acc * 16 + n)
            })
        }
    }
}

macro_rules! impl_ser_de_json_integer {
    ( $ty:ident, $reader:ident) => {
        impl SerJson for $ty {
            fn ser_json(&self, _d: usize, s: &mut SerJsonState) {
                _ = write!(s.out, "{self}");
            }
        }

        impl DeJson for $ty {
            fn de_json_value(s: &mut DeJsonScope<'_, '_>) -> Result<$ty, DeJsonErr> {
                s.$reader()
            }
        }
    };
}

macro_rules! impl_ser_de_json_float {
    ( $ty:ident, $reader:ident) => {
        impl SerJson for $ty {
            fn ser_json(&self, _d: usize, s: &mut SerJsonState) {
                _ = write!(s.out, "{self:?}");
            }
        }

        impl DeJson for $ty {
            fn de_json_value(s: &mut DeJsonScope<'_, '_>) -> Result<$ty, DeJsonErr> {
                s.$reader()
            }
        }
    };
}

impl_ser_de_json_integer!(usize, parse_usize);
impl_ser_de_json_integer!(u64, parse_u64);
impl_ser_de_json_integer!(u32, parse_u32);
impl_ser_de_json_integer!(u16, parse_u16);
impl_ser_de_json_integer!(u8, parse_u8);
impl_ser_de_json_integer!(isize, parse_isize);
impl_ser_de_json_integer!(i64, parse_i64);
impl_ser_de_json_integer!(i32, parse_i32);
impl_ser_de_json_integer!(i16, parse_i16);
impl_ser_de_json_integer!(i8, parse_i8);
impl_ser_de_json_float!(f64, parse_f64);
impl_ser_de_json_float!(f32, parse_f32);

impl<T> SerJson for Option<T>
where
    T: SerJson,
{
    fn ser_json(&self, d: usize, s: &mut SerJsonState) {
        if let Some(v) = self {
            v.ser_json(d, s);
        } else {
            s.out.push_str("null");
        }
    }
}

impl<T> DeJson for Option<T>
where
    T: DeJson,
{
    fn de_json_value(s: &mut DeJsonScope<'_, '_>) -> Result<Self, DeJsonErr> {
        Self::de_json(s)
    }
    fn de_json(s: &mut DeJsonScope<'_, '_>) -> Result<Self, DeJsonErr> {
        if let DeJsonTok::Null = s.tok {
            s.next_tok()?;
            return Ok(None);
        }
        Ok(Some(DeJson::de_json(s)?))
    }
}

impl SerJson for () {
    fn ser_json(&self, _d: usize, s: &mut SerJsonState) {
        s.out.push_str("null")
    }
}

impl DeJson for () {
    fn de_json_value(s: &mut DeJsonScope<'_, '_>) -> Result<(), DeJsonErr> {
        if let DeJsonTok::Null = s.tok {
            s.next_tok()?;
            Ok(())
        } else {
            Err(s.err_token("null"))
        }
    }
}

impl SerJson for bool {
    fn ser_json(&self, _d: usize, s: &mut SerJsonState) {
        if *self {
            s.out.push_str("true")
        } else {
            s.out.push_str("false")
        }
    }
}

impl DeJson for bool {
    fn de_json_value(s: &mut DeJsonScope<'_, '_>) -> Result<bool, DeJsonErr> {
        s.parse_bool()
    }
}

macro_rules! impl_ser_json_string {
    ($ty: ident) => {
        impl SerJson for $ty {
            fn ser_json(&self, _d: usize, s: &mut SerJsonState) {
                s.out.push('"');
                for c in self.chars() {
                    match c {
                        '\x08' => s.out += "\\b",
                        '\x0C' => s.out += "\\f",
                        '\n' => s.out += "\\n",
                        '\r' => s.out += "\\r",
                        '\t' => s.out += "\\t",
                        _ if c.is_ascii_control() => {
                            let _ = write!(s.out, "\\u{:04x}", c as u32);
                        }
                        '\\' => s.out += "\\\\",
                        '"' => s.out += "\\\"",
                        _ => s.out.push(c),
                    }
                }
                s.out.push('"');
            }
        }
    };
}

impl_ser_json_string!(String);
impl_ser_json_string!(str);

impl DeJson for String {
    fn de_json_value(s: &mut DeJsonScope<'_, '_>) -> Result<String, DeJsonErr> {
        s.parse_string()
    }
}

impl<T> SerJson for Vec<T>
where
    T: SerJson,
{
    fn ser_json(&self, d: usize, s: &mut SerJsonState) {
        s.out.push('[');
        if !self.is_empty() {
            let last = self.len() - 1;
            for (index, item) in self.iter().enumerate() {
                s.indent(d + 1);
                item.ser_json(d + 1, s);
                if index != last {
                    s.out.push(',');
                }
            }
        }
        s.out.push(']');
    }
}

impl<T> DeJson for Vec<T>
where
    T: DeJson,
{
    fn de_json_value(s: &mut DeJsonScope<'_, '_>) -> Result<Vec<T>, DeJsonErr> {
        let mut out = Vec::new();
        s.arr_open()?;

        while s.array_next_val()? {
            out.push(DeJson::de_json(s)?);
        }
        s.arr_close()?;
        Ok(out)
    }
}

#[cfg(feature = "std")]
impl<T> SerJson for std::collections::HashSet<T>
where
    T: SerJson,
{
    fn ser_json(&self, d: usize, s: &mut SerJsonState) {
        s.out.push('[');
        if !self.is_empty() {
            let last = self.len() - 1;
            for (index, item) in self.iter().enumerate() {
                s.indent(d + 1);
                item.ser_json(d + 1, s);
                if index != last {
                    s.out.push(',');
                }
            }
        }
        s.out.push(']');
    }
}

#[cfg(feature = "std")]
impl<T> DeJson for std::collections::HashSet<T>
where
    T: DeJson + core::hash::Hash + Eq,
{
    fn de_json_value(s: &mut DeJsonScope<'_, '_>) -> Result<Self, DeJsonErr> {
        let mut out = std::collections::HashSet::new();
        s.arr_open()?;

        while s.array_next_val()? {
            out.insert(DeJson::de_json(s)?);
        }
        s.arr_close()?;
        Ok(out)
    }
}

impl<T> SerJson for LinkedList<T>
where
    T: SerJson,
{
    fn ser_json(&self, d: usize, s: &mut SerJsonState) {
        s.out.push('[');
        if !self.is_empty() {
            let last = self.len() - 1;
            for (index, item) in self.iter().enumerate() {
                s.indent(d + 1);
                item.ser_json(d + 1, s);
                if index != last {
                    s.out.push(',');
                }
            }
        }
        s.out.push(']');
    }
}

impl<T> DeJson for LinkedList<T>
where
    T: DeJson,
{
    fn de_json_value(s: &mut DeJsonScope<'_, '_>) -> Result<LinkedList<T>, DeJsonErr> {
        let mut out = LinkedList::new();
        s.arr_open()?;

        while s.array_next_val()? {
            out.push_back(DeJson::de_json(s)?);
        }
        s.arr_close()?;
        Ok(out)
    }
}

impl<T> SerJson for BTreeSet<T>
where
    T: SerJson,
{
    fn ser_json(&self, d: usize, s: &mut SerJsonState) {
        s.out.push('[');
        if !self.is_empty() {
            let last = self.len() - 1;
            for (index, item) in self.iter().enumerate() {
                s.indent(d + 1);
                item.ser_json(d + 1, s);
                if index != last {
                    s.out.push(',');
                }
            }
        }
        s.out.push(']');
    }
}

impl<T> DeJson for BTreeSet<T>
where
    T: DeJson + Ord,
{
    fn de_json_value(s: &mut DeJsonScope<'_, '_>) -> Result<BTreeSet<T>, DeJsonErr> {
        let mut out = BTreeSet::new();
        s.arr_open()?;

        while s.array_next_val()? {
            out.insert(DeJson::de_json(s)?);
        }
        s.arr_close()?;
        Ok(out)
    }
}

impl<T> SerJson for [T]
where
    T: SerJson,
{
    fn ser_json(&self, d: usize, s: &mut SerJsonState) {
        s.out.push('[');
        let last = self.len() - 1;
        for (index, item) in self.iter().enumerate() {
            item.ser_json(d + 1, s);
            if index != last {
                s.out.push(',');
            }
        }
        s.out.push(']');
    }
}

impl<T, const N: usize> SerJson for [T; N]
where
    T: SerJson,
{
    #[inline(always)]
    fn ser_json(&self, d: usize, s: &mut SerJsonState) {
        self.as_slice().ser_json(d, s)
    }
}

impl<T, const N: usize> DeJson for [T; N]
where
    T: DeJson,
{
    fn de_json_value(o: &mut DeJsonScope<'_, '_>) -> Result<Self, DeJsonErr> {
        use ::core::mem::MaybeUninit;

        // waiting for uninit_array(or for array::try_from_fn) stabilization
        // https://github.com/rust-lang/rust/issues/96097
        // https://github.com/rust-lang/rust/issues/89379
        let mut to: [MaybeUninit<T>; N] =
            unsafe { MaybeUninit::<[MaybeUninit<T>; N]>::uninit().assume_init() };
        o.arr_open()?;

        for index in 0..N {
            to[index] = match (|| {
                if o.array_next_val()? {
                    DeJson::de_json(o)
                } else {
                    Err(o.err_token("array element"))
                }
            })() {
                Ok(v) => MaybeUninit::new(v),
                Err(e) => {
                    // drop all the MaybeUninit values which we've already
                    // successfully deserialized so we don't leak memory.
                    // See https://github.com/not-fl3/nanoserde/issues/79
                    for (_, to_drop) in (0..index).zip(to) {
                        unsafe { to_drop.assume_init() };
                    }
                    return Err(e);
                }
            }
        }

        // waiting for array_assume_init or core::array::map optimizations
        // https://github.com/rust-lang/rust/issues/61956
        // initializing before block close so that drop will run automatically if err encountered there
        let initialized =
            unsafe { (*(&to as *const _ as *const MaybeUninit<_>)).assume_init_read() };
        if o.array_next_val()? {
            return Err(o.err_token("]"));
        }
        o.arr_close()?;

        Ok(initialized)
    }
}

fn de_json_array_element<T>(s: &mut DeJsonScope<'_, '_>) -> Result<T, DeJsonErr>
where
    T: DeJson,
{
    if !s.array_next_val()? {
        return Err(s.err_token("array element"));
    }
    DeJson::de_json(s)
}

impl<A, B> SerJson for (A, B)
where
    A: SerJson,
    B: SerJson,
{
    fn ser_json(&self, d: usize, s: &mut SerJsonState) {
        s.out.push('[');
        self.0.ser_json(d, s);
        s.out.push(',');
        self.1.ser_json(d, s);
        s.out.push(']');
    }
}

impl<A, B> DeJson for (A, B)
where
    A: DeJson,
    B: DeJson,
{
    fn de_json_value(s: &mut DeJsonScope<'_, '_>) -> Result<(A, B), DeJsonErr> {
        s.arr_open()?;
        let r = (de_json_array_element(s)?, de_json_array_element(s)?);
        if s.array_next_val()? {
            return Err(s.err_token("]"));
        }
        s.arr_close()?;
        Ok(r)
    }
}

impl<A, B, C> SerJson for (A, B, C)
where
    A: SerJson,
    B: SerJson,
    C: SerJson,
{
    fn ser_json(&self, d: usize, s: &mut SerJsonState) {
        s.out.push('[');
        self.0.ser_json(d, s);
        s.out.push(',');
        self.1.ser_json(d, s);
        s.out.push(',');
        self.2.ser_json(d, s);
        s.out.push(']');
    }
}

impl<A, B, C> DeJson for (A, B, C)
where
    A: DeJson,
    B: DeJson,
    C: DeJson,
{
    fn de_json_value(s: &mut DeJsonScope<'_, '_>) -> Result<(A, B, C), DeJsonErr> {
        s.arr_open()?;
        let r = (
            de_json_array_element(s)?,
            de_json_array_element(s)?,
            de_json_array_element(s)?,
        );
        if s.array_next_val()? {
            return Err(s.err_token("]"));
        }
        s.arr_close()?;
        Ok(r)
    }
}

impl<A, B, C, D> SerJson for (A, B, C, D)
where
    A: SerJson,
    B: SerJson,
    C: SerJson,
    D: SerJson,
{
    fn ser_json(&self, d: usize, s: &mut SerJsonState) {
        s.out.push('[');
        self.0.ser_json(d, s);
        s.out.push(',');
        self.1.ser_json(d, s);
        s.out.push(',');
        self.2.ser_json(d, s);
        s.out.push(',');
        self.3.ser_json(d, s);
        s.out.push(']');
    }
}

impl<A, B, C, D> DeJson for (A, B, C, D)
where
    A: DeJson,
    B: DeJson,
    C: DeJson,
    D: DeJson,
{
    fn de_json_value(s: &mut DeJsonScope<'_, '_>) -> Result<(A, B, C, D), DeJsonErr> {
        s.arr_open()?;
        let r = (
            de_json_array_element(s)?,
            de_json_array_element(s)?,
            de_json_array_element(s)?,
            de_json_array_element(s)?,
        );
        if s.array_next_val()? {
            return Err(s.err_token("]"));
        }
        s.arr_close()?;
        Ok(r)
    }
}

#[cfg(feature = "std")]
impl<K, V, S> SerJson for std::collections::HashMap<K, V, S>
where
    K: SerJson,
    V: SerJson,
{
    fn ser_json(&self, d: usize, s: &mut SerJsonState) {
        s.out.push('{');
        let len = self.len();
        for (index, (k, v)) in self.iter().enumerate() {
            s.indent(d + 1);
            k.ser_json(d + 1, s);
            s.out.push(':');
            v.ser_json(d + 1, s);
            if (index + 1) < len {
                s.conl();
            }
        }
        s.indent(d);
        s.out.push('}');
    }
}

#[cfg(feature = "std")]
impl<K, V, S> DeJson for std::collections::HashMap<K, V, S>
where
    K: DeJson + Eq + core::hash::Hash,
    V: DeJson,
    S: core::hash::BuildHasher + Default,
{
    fn de_json_value(s: &mut DeJsonScope<'_, '_>) -> Result<Self, DeJsonErr> {
        let mut h = std::collections::HashMap::with_hasher(S::default());
        s.obj_open()?;
        while s.object_next_entry()? {
            let k = DeJson::de_json(s)?;
            s.colon()?;
            let v = DeJson::de_json(s)?;
            h.insert(k, v);
        }
        s.obj_close()?;
        Ok(h)
    }
}

impl<K, V> SerJson for BTreeMap<K, V>
where
    K: SerJson,
    V: SerJson,
{
    fn ser_json(&self, d: usize, s: &mut SerJsonState) {
        s.out.push('{');
        let len = self.len();
        for (index, (k, v)) in self.iter().enumerate() {
            s.indent(d + 1);
            k.ser_json(d + 1, s);
            s.out.push(':');
            v.ser_json(d + 1, s);
            if (index + 1) < len {
                s.conl();
            }
        }
        s.indent(d);
        s.out.push('}');
    }
}

impl<K, V> DeJson for BTreeMap<K, V>
where
    K: DeJson + Eq + Ord,
    V: DeJson,
{
    fn de_json_value(s: &mut DeJsonScope<'_, '_>) -> Result<Self, DeJsonErr> {
        let mut h = BTreeMap::new();
        s.obj_open()?;
        while s.object_next_entry()? {
            let k = DeJson::de_json(s)?;
            s.colon()?;
            let v = DeJson::de_json(s)?;
            h.insert(k, v);
        }
        s.obj_close()?;
        Ok(h)
    }
}

impl<T> SerJson for Box<T>
where
    T: SerJson,
{
    fn ser_json(&self, d: usize, s: &mut SerJsonState) {
        (**self).ser_json(d, s)
    }
}

impl<T> DeJson for Box<T>
where
    T: DeJson,
{
    fn de_json_value(s: &mut DeJsonScope<'_, '_>) -> Result<Self, DeJsonErr> {
        Self::de_json(s)
    }
    fn de_json(s: &mut DeJsonScope<'_, '_>) -> Result<Box<T>, DeJsonErr> {
        Ok(Box::new(DeJson::de_json(s)?))
    }
}

impl SerJson for Duration {
    fn ser_json(&self, _d: usize, s: &mut SerJsonState) {
        s.out.push('{');
        _ = write!(s.out, "\"secs\":{}", self.as_secs());
        if self.subsec_nanos() > 0 {
            _ = write!(s.out, ",\"nanos\":{}", self.subsec_nanos());
        }
        s.out.push('}');
    }
}

impl DeJson for Duration {
    fn de_json_value(s: &mut DeJsonScope<'_, '_>) -> Result<Duration, DeJsonErr> {
        let mut sec = None;
        let mut nanos = None;
        s.obj_open()?;
        while s.object_next_entry()? {
            let k: String = DeJson::de_json(s)?;
            match k.as_str() {
                "secs" => {
                    if sec.is_some() {
                        return Err(s.err_parse("Duration can only have one secs field"));
                    }
                    s.colon()?;
                    sec = Some(DeJson::de_json(s)?);
                }
                "nanos" => {
                    if nanos.is_some() {
                        return Err(s.err_parse("Duration can only have one nanos field"));
                    }
                    s.colon()?;
                    nanos = Some(DeJson::de_json(s)?);
                }
                _ => {
                    return Err(s.err_parse(&format!(
                        "Duration can only have secs or nanos fields, got {k}"
                    )));
                }
            }
        }
        s.obj_close()?;

        let secs = sec.ok_or_else(|| s.err_parse("Duration must have secs field"))?;
        let nanos = nanos.unwrap_or(0);

        if nanos > 1_000_000_000 {
            Err(s.err_range("Duration nanos must be at most 1,000,000,000"))
        } else {
            Ok(Duration::new(secs, nanos))
        }
    }
}

#[cfg(feature = "std")]
impl SerJson for std::time::SystemTime {
    fn ser_json(&self, d: usize, s: &mut SerJsonState) {
        self.duration_since(std::time::SystemTime::UNIX_EPOCH)
            .ok()
            .ser_json(d, s);
    }
}

#[cfg(feature = "std")]
impl DeJson for std::time::SystemTime {
    fn de_json_value(s: &mut DeJsonScope<'_, '_>) -> Result<Self, DeJsonErr> {
        Self::de_json(s)
    }
    fn de_json(s: &mut DeJsonScope<'_, '_>) -> Result<std::time::SystemTime, DeJsonErr> {
        match DeJson::de_json(s)? {
            Some(dur) => Ok(std::time::SystemTime::UNIX_EPOCH + dur),
            None => Ok(std::time::SystemTime::UNIX_EPOCH),
        }
    }
}

/// A JSON input structure. Children borrow shared tokenizer state independently.
pub struct DeJsonScope<'state, 'input> {
    state: &'state mut DeJsonState<'input>,
    opener: DeJsonTok,
    closer: DeJsonTok,
    done: bool,
    // An unused scope may finish after another scope reads the same value.
    can_finish: bool,
}

impl<'state, 'input> core::ops::Deref for DeJsonScope<'state, 'input> {
    type Target = DeJsonState<'input>;
    fn deref(&self) -> &Self::Target {
        self.state
    }
}

impl<'state, 'input> DeJsonScope<'state, 'input> {
    pub fn root(state: &'state mut DeJsonState<'input>) -> Self {
        Self {
            state,
            opener: DeJsonTok::Bof,
            closer: DeJsonTok::Eof,
            done: false,
            can_finish: true,
        }
    }

    pub fn new_scope(&mut self) -> Result<DeJsonScope<'_, 'input>, DeJsonErr> {
        let (opener, closer) = match self.token() {
            DeJsonTok::CurlyOpen => (DeJsonTok::CurlyOpen, DeJsonTok::CurlyClose),
            DeJsonTok::BlockOpen => (DeJsonTok::BlockOpen, DeJsonTok::BlockClose),
            _ => return Err(self.err_token("{ or [")),
        };
        if self.done {
            return Err(self.err_parse("scope already finished"));
        }
        Ok(DeJsonScope {
            state: &mut *self.state,
            opener,
            closer,
            done: false,
            can_finish: true,
        })
    }

    pub fn token(&self) -> &DeJsonTok {
        &self.state.tok
    }
    pub fn is_done(&self) -> bool {
        self.done
    }
    pub fn is_null(&self) -> bool {
        *self.token() == DeJsonTok::Null
    }
    pub fn is_string(&self) -> bool {
        *self.token() == DeJsonTok::Str
    }
    pub fn is_object_opener(&self) -> bool {
        *self.token() == DeJsonTok::CurlyOpen
    }
    pub fn is_array_opener(&self) -> bool {
        *self.token() == DeJsonTok::BlockOpen
    }

    /// Advance within this scope. Parent navigation consumes completed child closers.
    pub fn next_tok(&mut self) -> Result<(), DeJsonErr> {
        if self.done {
            return Err(self.err_parse("cannot advance a finished scope"));
        }
        self.can_finish = false;
        self.state.scan_token()?;
        if *self.token() == self.closer {
            self.done = true;
            self.can_finish = true;
        } else if matches!(
            self.token(),
            DeJsonTok::CurlyClose | DeJsonTok::BlockClose | DeJsonTok::Eof
        ) {
            return Err(self.err_token(&format!("{:?}", self.closer)));
        }
        Ok(())
    }

    pub fn check_token(&self, expected: &DeJsonTok) -> Result<(), DeJsonErr> {
        if self.token() == expected {
            Ok(())
        } else {
            Err(self.err_token(&format!("{expected:?}")))
        }
    }
    pub fn next_token_is(&mut self, expected: &DeJsonTok) -> Result<(), DeJsonErr> {
        self.next_tok()?;
        self.check_token(expected)
    }
    pub fn finish(&self) -> Result<(), DeJsonErr> {
        if self.can_finish && *self.token() == self.closer {
            Ok(())
        } else {
            Err(self.err_token(&format!("{:?}", self.closer)))
        }
    }

    pub fn obj_open(&mut self) -> Result<(), DeJsonErr> {
        self.check_token(&DeJsonTok::CurlyOpen)?;
        self.can_finish = false;
        Ok(())
    }
    pub fn arr_open(&mut self) -> Result<(), DeJsonErr> {
        self.check_token(&DeJsonTok::BlockOpen)?;
        self.can_finish = false;
        Ok(())
    }
    pub fn obj_close(&self) -> Result<(), DeJsonErr> {
        if self.closer != DeJsonTok::CurlyClose {
            return Err(self.err_token("object scope"));
        }
        self.finish()
    }
    pub fn arr_close(&self) -> Result<(), DeJsonErr> {
        if self.closer != DeJsonTok::BlockClose {
            return Err(self.err_token("array scope"));
        }
        self.finish()
    }

    fn next_entry(&mut self, object: bool) -> Result<bool, DeJsonErr> {
        let expected = if object {
            DeJsonTok::CurlyOpen
        } else {
            DeJsonTok::BlockOpen
        };
        if self.opener != expected {
            return Err(self.err_token("matching container scope"));
        }
        if self.done {
            return Ok(false);
        }
        if *self.token() == self.opener {
            self.next_tok()?;
            return Ok(!self.done);
        }
        if matches!(self.token(), DeJsonTok::CurlyClose | DeJsonTok::BlockClose) {
            self.next_tok()?;
            if self.done {
                return Ok(false);
            }
        }
        if *self.token() != DeJsonTok::Comma {
            return Err(self.err_sep());
        }
        self.next_tok()?;
        if self.done {
            return Err(self.err_sep());
        }
        Ok(true)
    }

    pub fn array_next_val(&mut self) -> Result<bool, DeJsonErr> {
        self.next_entry(false)
    }
    pub fn object_next_entry(&mut self) -> Result<bool, DeJsonErr> {
        self.next_entry(true)
    }

    pub fn next_key<'k>(&mut self, keys: &[&'k str]) -> Result<Option<&'k str>, DeJsonErr> {
        if !self.object_next_entry()? {
            return Ok(None);
        }
        self.check_token(&DeJsonTok::Str)?;
        let key = keys
            .iter()
            .copied()
            .find(|key| *key == self.state.strbuf)
            .ok_or_else(|| self.err_exp(&self.state.strbuf))?;
        self.next_colon()?;
        Ok(Some(key))
    }

    pub fn next_key_string(&mut self) -> Result<Option<String>, DeJsonErr> {
        if !self.object_next_entry()? {
            return Ok(None);
        }
        let key = self.parse_string()?;
        self.colon()?;
        Ok(Some(key))
    }

    pub fn colon(&mut self) -> Result<(), DeJsonErr> {
        self.check_token(&DeJsonTok::Colon)?;
        self.next_tok()?;
        if matches!(
            self.token(),
            DeJsonTok::Comma
                | DeJsonTok::Colon
                | DeJsonTok::CurlyClose
                | DeJsonTok::BlockClose
                | DeJsonTok::Eof
        ) {
            return Err(self.err_token("JSON value"));
        }
        Ok(())
    }
    pub fn next_colon(&mut self) -> Result<(), DeJsonErr> {
        self.next_tok()?;
        self.colon()
    }

    /// Read an enum's single field, accepting nanoserde's older `[value]` wrapper.
    #[doc(hidden)]
    pub fn parse_variant_value<T: DeJson>(&mut self) -> Result<T, DeJsonErr> {
        // Try the current wire representation first, so Vec/tuple/aliased fields
        // get their own array scope. Only try the legacy wrapper if no input was
        // consumed and the original array opener is still current. No rewind or
        // cloned input buffers are needed.
        let before = (self.state.input.as_str().len(), self.state.cur);
        match T::de_json(self) {
            Err(_)
                if self.is_array_opener()
                    && before == (self.state.input.as_str().len(), self.state.cur) =>
            {
                let mut child = self.new_scope()?;
                if !child.array_next_val()? {
                    return Err(child.err_token("array element"));
                }
                let value = T::de_json(&mut child)?;
                if child.array_next_val()? {
                    return Err(child.err_token("]"));
                }
                child.arr_close()?;
                Ok(value)
            }
            result => result,
        }
    }

    pub fn skip_value(&mut self) -> Result<(), DeJsonErr> {
        match self.token() {
            DeJsonTok::CurlyOpen => {
                let mut child = self.new_scope()?;
                while child.next_key_string()?.is_some() {
                    child.skip_value()?;
                }
                child.obj_close()
            }
            DeJsonTok::BlockOpen => {
                let mut child = self.new_scope()?;
                while child.array_next_val()? {
                    child.skip_value()?;
                }
                child.arr_close()
            }
            DeJsonTok::Str
            | DeJsonTok::U64(_)
            | DeJsonTok::I64(_)
            | DeJsonTok::F64(_)
            | DeJsonTok::Bool(_)
            | DeJsonTok::Null => self.next_tok(),
            _ => Err(self.err_token("JSON value")),
        }
    }

    pub fn parse_string(&mut self) -> Result<String, DeJsonErr> {
        let value = self.as_string()?;
        self.next_tok()?;
        Ok(value)
    }
    pub fn parse_bool(&mut self) -> Result<bool, DeJsonErr> {
        let value = self.as_bool()?;
        self.next_tok()?;
        Ok(value)
    }
    pub fn parse_null(&mut self) -> Result<(), DeJsonErr> {
        self.check_token(&DeJsonTok::Null)?;
        self.next_tok()
    }
    fn u64_range(&mut self, max: u64) -> Result<u64, DeJsonErr> {
        if let DeJsonTok::U64(value) = self.tok {
            if value > max {
                return Err(self.err_range(&format!("{}>{}", value, max)));
            }
            return Ok(value);
        }
        Err(self.err_token("unsigned integer"))
    }

    fn i64_range(&mut self, min: i64, max: i64) -> Result<i64, DeJsonErr> {
        if let DeJsonTok::I64(value) = self.tok {
            if value < min {
                return Err(self.err_range(&format!("{}<{}", value, min)));
            }
            if value > max {
                return Err(self.err_range(&format!("{}>{}", value, max)));
            }
            return Ok(value);
        }
        if let DeJsonTok::U64(value) = self.tok {
            if value > max as u64 {
                return Err(self.err_range(&format!("{}>{}", value, max)));
            }
            return Ok(value as i64);
        }
        Err(self.err_token("signed integer"))
    }

    fn as_f64(&mut self) -> Result<f64, DeJsonErr> {
        if let DeJsonTok::I64(value) = self.tok {
            return Ok(value as f64);
        }
        if let DeJsonTok::U64(value) = self.tok {
            return Ok(value as f64);
        }
        if let DeJsonTok::F64(value) = self.tok {
            return Ok(value);
        }
        Err(self.err_token("floating point"))
    }

    fn as_bool(&mut self) -> Result<bool, DeJsonErr> {
        if let DeJsonTok::Bool(value) = self.tok {
            return Ok(value);
        }
        Err(self.err_token("boolean"))
    }

    fn as_string(&mut self) -> Result<String, DeJsonErr> {
        if let DeJsonTok::Str = &self.state.tok {
            let mut val = String::new();
            core::mem::swap(&mut val, &mut self.state.strbuf);
            return Ok(val);
        }
        Err(self.err_token("string"))
    }
}

macro_rules! scope_unsigned { ($($name:ident: $ty:ty),* $(,)?) => { impl DeJsonScope<'_, '_> { $(
    pub fn $name(&mut self) -> Result<$ty, DeJsonErr> { let value = self.u64_range(<$ty>::MAX as u64)?; self.next_tok()?; Ok(value as $ty) }
)* } }; }
macro_rules! scope_signed { ($($name:ident: $ty:ty),* $(,)?) => { impl DeJsonScope<'_, '_> { $(
    pub fn $name(&mut self) -> Result<$ty, DeJsonErr> { let value = self.i64_range(<$ty>::MIN as i64, <$ty>::MAX as i64)?; self.next_tok()?; Ok(value as $ty) }
)* } }; }
scope_unsigned!(parse_u8: u8, parse_u16: u16, parse_u32: u32, parse_u64: u64, parse_usize: usize);
scope_signed!(parse_i8: i8, parse_i16: i16, parse_i32: i32, parse_i64: i64, parse_isize: isize);
impl DeJsonScope<'_, '_> {
    pub fn parse_f64(&mut self) -> Result<f64, DeJsonErr> {
        let value = self.as_f64()?;
        self.next_tok()?;
        Ok(value)
    }
    pub fn parse_f32(&mut self) -> Result<f32, DeJsonErr> {
        self.parse_f64().map(|value| value as f32)
    }
}
