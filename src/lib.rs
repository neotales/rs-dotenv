//! Source-preserving, policy-free parsing for dotenv documents.
#![doc = include_str!("../README.md")]

use std::fmt;

use indexmap::IndexMap;

const MISSING_ASSIGNMENT: &str = "expected KEY=VALUE assignment in dotenv file";
const UNTERMINATED_VALUE: &str = "unterminated quoted dotenv value";
const TRAILING_GARBAGE: &str = "unexpected characters after a quoted dotenv value";

/// A one-based source location within a dotenv document.
///
/// A node that was added by a document method rather than parsed from source carries an
/// all-zero span.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Span {
    /// First source line covered by the node.
    pub line: u32,
    /// First source column covered by the node.
    pub column: u32,
    /// Last source line covered by the node.
    pub end_line: u32,
    /// Last source column covered by the node.
    pub end_column: u32,
}

/// The quoting syntax used by an assignment value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QuoteStyle {
    /// An unquoted value.
    Unquoted,
    /// A single-quoted value.
    Single,
    /// A double-quoted value.
    Double,
    /// A backtick-delimited raw value.
    Raw,
}

/// One element of a dotenv document, in source order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Node {
    /// A blank source line.
    Blank {
        /// Where the line is in the source document.
        span: Span,
    },
    /// A comment-only source line.
    Comment {
        /// The comment text, without its leading `#`.
        text: String,
        /// Where the line is in the source document.
        span: Span,
    },
    /// An environment assignment.
    ///
    /// An inline comment on the line is part of the assignment, not a separate node.
    Assignment(Assignment),
    /// Source that could not be interpreted as an assignment.
    Invalid {
        /// The source text that could not be parsed, which may span several lines.
        text: String,
        /// A description of what was expected instead.
        message: String,
        /// Where the offending source begins and ends.
        span: Span,
    },
}

/// A parsed environment assignment.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Assignment {
    /// Whether the source used the optional `export` prefix.
    pub exported: bool,
    /// The unvalidated assignment name.
    pub key: String,
    /// The decoded value with only its outer quotes removed.
    pub value: String,
    /// The source quote syntax.
    pub quote: QuoteStyle,
    /// The text of a trailing comment on the same line, without its leading `#`.
    pub inline_comment: Option<String>,
    /// Assignment source span.
    pub span: Span,
}

/// A dotenv document: an ordered element list plus a map of the key-value pairs.
///
/// The element list keeps the source order, the blank lines, the comments, and the quoting, and
/// the [`Display`](fmt::Display) implementation walks it to write the file back. The map answers
/// [`Document::get`], [`Document::keys`], and [`Document::to_map`] with one entry per key; a key
/// that appears twice in the source updates the map value it already has.
///
/// A document is either parsed from text with [`parse`] or built with [`Document::new`] and the
/// `add_*` methods.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Document {
    source: String,
    nodes: Vec<Node>,
    map: IndexMap<String, String>,
    trailing_newline: bool,
}

impl Document {
    /// Creates an empty document with no nodes and no keys.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the text the document was parsed from, unchanged by later edits.
    ///
    /// A document built with [`Document::new`] has an empty source. The `to_string()` call
    /// returns the text the document currently represents.
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Returns the elements in source order.
    #[must_use]
    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }

    /// Returns the number of elements in the element list.
    #[must_use]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Returns whether the document has no elements.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Returns the element at `index`, or `None` when the index is out of range.
    #[must_use]
    pub fn at(&self, index: usize) -> Option<&Node> {
        self.nodes.get(index)
    }

    /// Appends a blank line.
    pub fn add_newline(&mut self) {
        self.nodes.push(Node::Blank {
            span: Span::default(),
        });
    }

    /// Appends a comment on its own line.
    ///
    /// `text` is stored without a leading `#`, which [`Display`](fmt::Display) adds back.
    pub fn add_comment(&mut self, text: &str) {
        self.nodes.push(Node::Comment {
            text: text.to_owned(),
            span: Span::default(),
        });
    }

    /// Attaches a comment to the end of the preceding variable's line.
    ///
    /// A second call joins the text to the comment already there with a space. When the last node
    /// is not a variable, the text becomes a full-line comment instead.
    pub fn add_inline_comment(&mut self, text: &str) {
        if let Some(Node::Assignment(assignment)) = self.nodes.last_mut() {
            match &mut assignment.inline_comment {
                Some(existing) => {
                    existing.push(' ');
                    existing.push_str(text);
                }
                None => assignment.inline_comment = Some(text.to_owned()),
            }
            return;
        }
        self.add_comment(text);
    }

    /// Appends a variable, quoting the value when dotenv syntax requires it.
    ///
    /// A value that is already wrapped in `"` or `'` is stored with that quote and the wrapping
    /// removed. Otherwise the value is quoted with `"` when it contains a character that would
    /// not survive unquoted, such as `#`, a quote, a newline, or a tab.
    ///
    /// A key that is already in the document keeps its position and takes the new value instead
    /// of gaining a second element. The line is written without an `export` prefix; use
    /// [`Document::add_exported_variable`] for that.
    pub fn add_variable(&mut self, key: &str, value: &str) {
        self.add_variable_with(key, value, false);
    }

    /// Appends a variable on an `export KEY=value` line.
    ///
    /// Quoting works as it does for [`Document::add_variable`], and an existing key keeps its
    /// position and takes the new value and the `export` prefix.
    pub fn add_exported_variable(&mut self, key: &str, value: &str) {
        self.add_variable_with(key, value, true);
    }

    fn add_variable_with(&mut self, key: &str, value: &str, exported: bool) {
        if let Some(first @ ('"' | '\'')) = value.chars().next()
            && value.len() >= 2
            && value.ends_with(first)
        {
            let inner = &value[first.len_utf8()..value.len() - 1];
            self.upsert(key, inner, quote_style(first), exported);
            return;
        }
        self.upsert(key, value, auto_quote(value), exported);
    }

    /// Appends a variable with an explicit quote style.
    ///
    /// [`QuoteStyle::Unquoted`] writes the value as given, so a value containing `#`, a newline,
    /// or leading whitespace will not read back unchanged. A key that is already in the document
    /// keeps its position and takes the new value and quote.
    pub fn add_quoted_variable(&mut self, key: &str, value: &str, quote: QuoteStyle) {
        self.upsert(key, value, quote, false);
    }

    /// Returns the value assigned to `key` from the map, or `None`.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&str> {
        self.map.get(key).map(String::as_str)
    }

    /// Returns every key in insertion order, one per key.
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.map.keys().map(String::as_str)
    }

    /// Returns comment text in source order, both full-line and inline, without the leading `#`.
    pub fn comments(&self) -> impl Iterator<Item = &str> {
        self.nodes.iter().filter_map(|node| match node {
            Node::Comment { text, .. } => Some(text.as_str()),
            Node::Assignment(assignment) => assignment.inline_comment.as_deref(),
            Node::Blank { .. } | Node::Invalid { .. } => None,
        })
    }

    /// Replaces the value of `key` in the map and in its variable elements, or appends the key.
    ///
    /// An existing key keeps its position. Every variable element with that key is updated, so a
    /// duplicate source line does not keep an older value. An existing quote style is kept; an
    /// unquoted value that needs quoting is quoted so the document still writes valid dotenv.
    pub fn set(&mut self, key: &str, value: &str) {
        if let Some(existing) = self.map.get_mut(key) {
            value.clone_into(existing);
        } else {
            let quote = auto_quote(value);
            self.upsert(key, value, quote, false);
            return;
        }
        for node in &mut self.nodes {
            if let Node::Assignment(assignment) = node
                && assignment.key == key
            {
                value.clone_into(&mut assignment.value);
                if assignment.quote == QuoteStyle::Unquoted {
                    assignment.quote = auto_quote(value);
                }
            }
        }
    }

    /// Applies every key in `other` with [`Document::set`], in `other`'s order.
    ///
    /// An existing key is overwritten in place and a missing key is appended. Comments, quoting,
    /// and blank lines from `other` are not copied.
    pub fn merge(&mut self, other: &Document) {
        for (key, value) in &other.map {
            self.set(key, value);
        }
    }

    /// Returns the key-value map, one entry per key in insertion order.
    ///
    /// A key that appeared twice in the source has the later value here. Quote style and comments
    /// stay with the elements, so the map does not describe them.
    #[must_use]
    pub fn to_map(&self) -> &IndexMap<String, String> {
        &self.map
    }

    /// Returns assignments in source order, excluding the other node kinds.
    ///
    /// This is a low-level view of the element list. Use [`Document::keys`] and [`Document::get`]
    /// to read a document.
    pub fn assignments(&self) -> impl Iterator<Item = &Assignment> {
        self.nodes.iter().filter_map(|node| match node {
            Node::Assignment(assignment) => Some(assignment),
            Node::Blank { .. } | Node::Comment { .. } | Node::Invalid { .. } => None,
        })
    }

    /// Returns the first recoverable parse error, if any.
    #[must_use]
    pub fn first_invalid(&self) -> Option<(&str, &Span)> {
        self.nodes.iter().find_map(|node| match node {
            Node::Invalid { message, span, .. } => Some((message.as_str(), span)),
            Node::Blank { .. } | Node::Comment { .. } | Node::Assignment(_) => None,
        })
    }

    fn upsert(&mut self, key: &str, value: &str, quote: QuoteStyle, exported: bool) {
        let existed = self.map.insert(key.to_owned(), value.to_owned()).is_some();
        if !existed {
            self.nodes.push(Node::Assignment(Assignment {
                exported,
                key: key.to_owned(),
                value: value.to_owned(),
                quote,
                inline_comment: None,
                span: Span::default(),
            }));
            return;
        }
        for node in &mut self.nodes {
            if let Node::Assignment(assignment) = node
                && assignment.key == key
            {
                value.clone_into(&mut assignment.value);
                assignment.quote = quote;
                assignment.exported = exported;
            }
        }
    }

    fn insert_parsed(&mut self, key: &str, assignment: Assignment) {
        self.map.insert(key.to_owned(), assignment.value.clone());
        self.nodes.push(Node::Assignment(assignment));
    }
}

impl fmt::Display for Document {
    /// Writes the document back as dotenv text.
    ///
    /// A variable is `KEY=value`, a quoted value keeps its quote, a full-line comment is
    /// `# text`, and an inline comment stays on its variable's line as ` # text`.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut out = String::new();
        for (position, node) in self.nodes.iter().enumerate() {
            if position > 0 {
                out.push('\n');
            }
            match node {
                Node::Blank { .. } => {}
                Node::Comment { text, .. } => push_comment(&mut out, text),
                Node::Invalid { text, .. } => out.push_str(text),
                Node::Assignment(assignment) => push_assignment(&mut out, assignment),
            }
        }
        if self.trailing_newline && !self.nodes.is_empty() {
            out.push('\n');
        }
        formatter.write_str(&out)
    }
}

/// Parses a dotenv document without interpolation, key validation, or I/O.
#[must_use]
pub fn parse(input: &str) -> Document {
    let mut document = Document {
        source: input.to_owned(),
        trailing_newline: input.ends_with('\n'),
        ..Document::default()
    };
    let lines: Vec<&str> = input.split_inclusive('\n').collect();
    let mut index = 0;

    while index < lines.len() {
        let line_number = u32::try_from(index + 1).unwrap_or(u32::MAX);
        let raw = line_body(lines[index]);
        let trimmed = raw.trim();
        let span = line_span(line_number, raw);

        if trimmed.is_empty() {
            document.nodes.push(Node::Blank { span });
            index += 1;
            continue;
        }
        if trimmed.starts_with('#') {
            document.nodes.push(Node::Comment {
                text: comment_text(trimmed),
                span,
            });
            index += 1;
            continue;
        }

        let (exported, body) = match raw.trim_start().strip_prefix("export") {
            Some(rest) if rest.starts_with(char::is_whitespace) => (true, rest.trim_start()),
            _ => (false, raw.trim_start()),
        };
        let Some((key, value_text)) = body.split_once('=') else {
            document.nodes.push(Node::Invalid {
                text: raw.to_owned(),
                message: MISSING_ASSIGNMENT.to_owned(),
                span,
            });
            index += 1;
            continue;
        };
        let key = key.trim();
        if key.is_empty() {
            document.nodes.push(Node::Invalid {
                text: raw.to_owned(),
                message: MISSING_ASSIGNMENT.to_owned(),
                span,
            });
            index += 1;
            continue;
        }

        index = push_value(
            &mut document,
            key,
            exported,
            line_number,
            &lines,
            index,
            value_text.trim_start(),
        );
    }

    document
}

/// Adds one parsed assignment to the document and returns the index of the next line.
fn push_value(
    document: &mut Document,
    key: &str,
    exported: bool,
    line_number: u32,
    lines: &[&str],
    index: usize,
    value_text: &str,
) -> usize {
    match parse_value(value_text, lines, index) {
        Parsed::Plain {
            value,
            inline_comment,
        } => {
            document.insert_parsed(
                key,
                Assignment {
                    exported,
                    key: key.to_owned(),
                    value,
                    quote: QuoteStyle::Unquoted,
                    inline_comment,
                    span: line_span(line_number, line_body(lines[index])),
                },
            );
            index + 1
        }
        Parsed::Quoted {
            quote,
            value,
            inline_comment,
            end_index,
        } => {
            document.insert_parsed(
                key,
                Assignment {
                    exported,
                    key: key.to_owned(),
                    value,
                    quote,
                    inline_comment,
                    span: multi_line_span(line_number, end_index, lines),
                },
            );
            end_index + 1
        }
        Parsed::Unterminated { end_index } => {
            document.nodes.push(Node::Invalid {
                text: input_line_range(lines, index, end_index),
                message: UNTERMINATED_VALUE.to_owned(),
                span: multi_line_span(line_number, end_index, lines),
            });
            end_index + 1
        }
        Parsed::TrailingGarbage { end_index } => {
            document.nodes.push(Node::Invalid {
                text: input_line_range(lines, index, end_index),
                message: TRAILING_GARBAGE.to_owned(),
                span: multi_line_span(line_number, end_index, lines),
            });
            end_index + 1
        }
    }
}

/// A value as it appeared on the first line of an assignment.
enum Parsed {
    Plain {
        value: String,
        inline_comment: Option<String>,
    },
    Quoted {
        quote: QuoteStyle,
        value: String,
        inline_comment: Option<String>,
        end_index: usize,
    },
    Unterminated {
        end_index: usize,
    },
    TrailingGarbage {
        end_index: usize,
    },
}

/// A chunk of a quoted value: either closed on this line, or still open at its end.
enum Scanned {
    Closed { text: String, rest: usize },
    Open { text: String },
}

fn parse_value(value_text: &str, lines: &[&str], start: usize) -> Parsed {
    let Some(first) = value_text.chars().next() else {
        return Parsed::Plain {
            value: String::new(),
            inline_comment: None,
        };
    };
    if !matches!(first, '"' | '\'' | '`') {
        let (value, inline_comment) = match value_text.split_once('#') {
            Some((value, comment)) => (value.trim().to_owned(), Some(comment.trim().to_owned())),
            None => (value_text.trim().to_owned(), None),
        };
        return Parsed::Plain {
            value,
            inline_comment,
        };
    }

    let quote = quote_style(first);
    let mut value = String::new();
    let mut end_index = start;
    let mut chunk = &value_text[first.len_utf8()..];
    let mut tail = None;
    loop {
        match scan_quoted(quote, chunk) {
            Scanned::Closed { text, rest } => {
                value.push_str(&text);
                tail = Some(chunk[rest..].to_owned());
                break;
            }
            Scanned::Open { text } => {
                value.push_str(&text);
                if end_index + 1 >= lines.len() {
                    break;
                }
                end_index += 1;
                value.push('\n');
                chunk = line_body(lines[end_index]);
            }
        }
    }
    let Some(tail) = tail else {
        return Parsed::Unterminated { end_index };
    };

    let tail = tail.trim();
    if tail.is_empty() {
        return Parsed::Quoted {
            quote,
            value,
            inline_comment: None,
            end_index,
        };
    }
    match tail.strip_prefix('#') {
        Some(comment) => Parsed::Quoted {
            quote,
            value,
            inline_comment: Some(comment.trim().to_owned()),
            end_index,
        },
        None => Parsed::TrailingGarbage { end_index },
    }
}

/// Scans one chunk of a quoted value, decoding escapes and stopping at the closing delimiter.
fn scan_quoted(quote: QuoteStyle, chunk: &str) -> Scanned {
    let delimiter = delimiter(quote);
    let mut text = String::with_capacity(chunk.len());
    let mut chars = chunk.char_indices().peekable();

    while let Some((offset, character)) = chars.next() {
        if character == delimiter {
            return Scanned::Closed {
                text,
                rest: offset + character.len_utf8(),
            };
        }
        if character != '\\' {
            text.push(character);
            continue;
        }

        let Some(&(_, next)) = chars.peek() else {
            text.push('\\');
            continue;
        };

        if quote == QuoteStyle::Single {
            match next {
                '\\' => {
                    text.push('\\');
                    chars.next();
                }
                '\'' => {
                    text.push('\'');
                    chars.next();
                }
                _ => text.push('\\'),
            }
            continue;
        }

        let escaped = match next {
            'n' => Some('\n'),
            'r' => Some('\r'),
            't' => Some('\t'),
            'b' => Some('\u{8}'),
            '\\' => Some('\\'),
            '"' => Some('"'),
            '\'' => Some('\''),
            '`' => Some('`'),
            _ => None,
        };
        if let Some(decoded) = escaped {
            text.push(decoded);
            chars.next();
            continue;
        }
        if (next == 'u' || next == 'U')
            && let Some((decoded, width)) = unicode_escape(&chars, next)
        {
            text.push(decoded);
            for _ in 0..=width {
                chars.next();
            }
            continue;
        }
        text.push('\\');
    }

    Scanned::Open { text }
}

/// Reads the hex digits of a `\uXXXX` or `\UXXXXXXXX` escape that follows the `u` or `U`.
fn unicode_escape(
    chars: &std::iter::Peekable<std::str::CharIndices<'_>>,
    kind: char,
) -> Option<(char, usize)> {
    let width = if kind == 'u' { 4 } else { 8 };
    let mut probe = chars.clone();
    probe.next();
    let mut digits = String::with_capacity(width);
    while digits.len() < width {
        match probe.peek() {
            Some(&(_, digit)) if digit.is_ascii_hexdigit() => {
                digits.push(digit);
                probe.next();
            }
            _ => return None,
        }
    }
    let code = u32::from_str_radix(&digits, 16).ok()?;
    Some((char::from_u32(code)?, width))
}

fn push_comment(out: &mut String, text: &str) {
    out.push('#');
    if !text.is_empty() {
        out.push(' ');
        out.push_str(text);
    }
}

fn push_assignment(out: &mut String, assignment: &Assignment) {
    if assignment.exported {
        out.push_str("export ");
    }
    out.push_str(&assignment.key);
    out.push('=');
    if assignment.quote == QuoteStyle::Unquoted {
        out.push_str(&assignment.value);
    } else {
        let delimiter = delimiter(assignment.quote);
        out.push(delimiter);
        push_quoted(out, &assignment.value, assignment.quote);
        out.push(delimiter);
    }
    if let Some(comment) = &assignment.inline_comment {
        out.push_str(" #");
        if !comment.is_empty() {
            out.push(' ');
            out.push_str(comment);
        }
    }
}

/// Writes a value inside its quotes, escaping only what the matching scanner would misread.
fn push_quoted(out: &mut String, value: &str, quote: QuoteStyle) {
    let delimiter = delimiter(quote);
    for character in value.chars() {
        if character == delimiter {
            out.push('\\');
            out.push(character);
            continue;
        }
        match quote {
            QuoteStyle::Single => match character {
                '\\' => out.push_str("\\\\"),
                _ => out.push(character),
            },
            QuoteStyle::Unquoted => out.push(character),
            QuoteStyle::Double | QuoteStyle::Raw => match character {
                // A real newline keeps a multi-line value on its original lines. Every other
                // control character is written as an escape so the line structure survives.
                '\\' => out.push_str("\\\\"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                '\u{8}' => out.push_str("\\b"),
                _ => out.push(character),
            },
        }
    }
}

fn quote_style(delimiter: char) -> QuoteStyle {
    match delimiter {
        '\'' => QuoteStyle::Single,
        '`' => QuoteStyle::Raw,
        _ => QuoteStyle::Double,
    }
}

fn delimiter(quote: QuoteStyle) -> char {
    match quote {
        QuoteStyle::Single => '\'',
        QuoteStyle::Double => '"',
        QuoteStyle::Raw => '`',
        QuoteStyle::Unquoted => '\0',
    }
}

/// Picks the quote a value needs to survive a round trip, or [`QuoteStyle::Unquoted`].
fn auto_quote(value: &str) -> QuoteStyle {
    if value != value.trim() {
        return QuoteStyle::Double;
    }
    let mut characters = value.chars().peekable();
    while let Some(character) = characters.next() {
        if matches!(
            character,
            '"' | '\'' | '\n' | '\r' | '\t' | '=' | '#' | '\u{8}' | '\u{c}' | '\u{b}'
        ) {
            return QuoteStyle::Double;
        }
        if character == '\\'
            && let Some(&next) = characters.peek()
            && matches!(next, '\\' | 'n' | 'r' | 't' | 'u' | 'U' | 'b' | 'f')
        {
            return QuoteStyle::Double;
        }
    }
    QuoteStyle::Unquoted
}

fn comment_text(trimmed: &str) -> String {
    trimmed
        .strip_prefix('#')
        .unwrap_or(trimmed)
        .trim()
        .to_owned()
}

fn line_body(line: &str) -> &str {
    let body = line.strip_suffix('\n').unwrap_or(line);
    body.strip_suffix('\r').unwrap_or(body)
}

fn line_span(line: u32, source: &str) -> Span {
    let width = u32::try_from(source.chars().count() + 1).unwrap_or(u32::MAX);
    Span {
        line,
        column: 1,
        end_line: line,
        end_column: width,
    }
}

fn multi_line_span(start: u32, end_index: usize, lines: &[&str]) -> Span {
    let end_line = u32::try_from(end_index + 1).unwrap_or(u32::MAX);
    let raw = line_body(lines[end_index]);
    Span {
        line: start,
        column: 1,
        end_line,
        end_column: u32::try_from(raw.chars().count() + 1).unwrap_or(u32::MAX),
    }
}

fn input_line_range(lines: &[&str], start: usize, end: usize) -> String {
    lines[start..=end].concat()
}

#[cfg(test)]
mod tests {
    use super::{Node, QuoteStyle, parse};

    #[test]
    fn preserves_document_structure_and_crlf_source() {
        let input = "# note\r\n\r\nexport NAME='Neo Env'\r\nCOUNT=2\r\n";
        let document = parse(input);
        assert_eq!(document.source(), input);
        assert!(matches!(document.nodes()[0], Node::Comment { .. }));
        assert!(matches!(document.nodes()[1], Node::Blank { .. }));
        let assignments: Vec<_> = document.assignments().collect();
        assert_eq!(assignments[0].value, "Neo Env");
        assert_eq!(assignments[0].quote, QuoteStyle::Single);
        assert_eq!(assignments[1].value, "2");
    }

    #[test]
    fn supports_multiline_and_raw_quoted_values() {
        let document = parse("DOUBLE=\"one\ntwo\"\nRAW=`three`\n");
        let assignments: Vec<_> = document.assignments().collect();
        assert_eq!(assignments[0].value, "one\ntwo");
        assert_eq!(assignments[0].span.end_line, 2);
        assert_eq!(assignments[1].quote, QuoteStyle::Raw);
    }

    #[test]
    fn keeps_value_syntax_without_interpolation() {
        let input = "# retained\r\nSINGLE='a $NAME'\r\nDOUBLE=\"b ${NAME}\"\r\nRAW=`c $NAME`\r\nPLAIN=$NAME # literal\r\nMULTI=\"first\r\nsecond\"\r\n";
        let document = parse(input);
        let assignments: Vec<_> = document.assignments().collect();
        assert_eq!(document.source(), input);
        assert_eq!(assignments.len(), 5);
        assert_eq!(assignments[0].value, "a $NAME");
        assert_eq!(assignments[1].value, "b ${NAME}");
        assert_eq!(assignments[2].value, "c $NAME");
        assert_eq!(assignments[3].value, "$NAME");
        assert_eq!(assignments[3].inline_comment.as_deref(), Some("literal"));
        assert_eq!(assignments[4].value, "first\nsecond");
        assert_eq!(assignments[4].span.end_line, 7);
    }

    #[test]
    fn a_duplicate_key_updates_the_map_but_keeps_both_elements() {
        let mut document = parse("A=1\nB=2\nA=3\n");
        assert_eq!(document.keys().collect::<Vec<_>>(), ["A", "B"]);
        assert_eq!(document.get("A"), Some("3"));
        assert_eq!(document.assignments().count(), 3);
        assert_eq!(document.to_string(), "A=1\nB=2\nA=3\n");

        document.set("A", "9");
        assert_eq!(document.get("A"), Some("9"));
        assert_eq!(document.to_string(), "A=9\nB=2\nA=9\n");
    }

    #[test]
    fn retains_invalid_lines_for_callers_to_report() {
        let document = parse("NAME=value\nbroken\n");
        assert_eq!(
            document.first_invalid().map(|(message, _)| message),
            Some("expected KEY=VALUE assignment in dotenv file")
        );
        assert_eq!(document.get("NAME"), Some("value"));
        assert_eq!(document.to_string(), "NAME=value\nbroken\n");
    }
}
