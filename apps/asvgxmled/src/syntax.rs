use repose_core::text::{AnnotatedString, AnnotatedStringBuilder, IdentityOffsetMapping, SpanStyle, TransformedText, VisualTransformation};
use repose_core::Color;
use vector_model::Rgba;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Palette {
    Light,
    Dark,
}

impl Palette {
    fn tag(self) -> Color { self.pick("#7C3AED", "#C4B5FD") }
    fn attr(self) -> Color { self.pick("#0F766E", "#5EEAD4") }
    fn value(self) -> Color { self.pick("#B45309", "#FCD34D") }
    fn comment(self) -> Color { self.pick("#6B7280", "#9CA3AF") }
    fn punct(self) -> Color { self.pick("#374151", "#D1D5DB") }
    fn entity(self) -> Color { self.pick("#BE123C", "#FDA4AF") }
    fn text(self) -> Color { self.pick("#1F2937", "#E5E7EB") }

    fn pick(self, light: &str, dark: &str) -> Color {
        Color::from_hex(if self == Self::Light { light } else { dark })
    }
}

#[derive(Debug)]
pub struct XmlHighlighter {
    palette: Palette,
}

impl XmlHighlighter {
    pub fn new(palette: Palette) -> Self {
        Self { palette }
    }

    fn color_value(b: &mut AnnotatedStringBuilder, value: &str, p: Palette) {
        if let Some(hex) = value.strip_prefix('#') {
            if let Some(rgba) = Rgba::parse(&format!("#{hex}")) {
                b.push_color("#", p.value());
                b.push_color(hex, Color::from_rgba(rgba.r, rgba.g, rgba.b, 255));
                return;
            }
        }
        if value.starts_with('@') || value.starts_with('?') {
            b.push_color(value, p.entity());
        } else {
            b.push_color(value, p.value());
        }
    }
}

impl VisualTransformation for XmlHighlighter {
    fn filter(&self, text: &AnnotatedString) -> TransformedText {
        let src = text.as_str();
        let p = self.palette;
        let mut b = AnnotatedStringBuilder::new();
        let bytes = src.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if src[i..].starts_with("<!--") {
                let end = src[i + 4..].find("-->").map(|n| i + 4 + n + 3).unwrap_or(bytes.len());
                b.push_color(&src[i..end], p.comment());
                i = end;
                continue;
            }
            if bytes[i] != b'<' {
                let end = src[i..].find(['<', '&']).map(|n| i + n).unwrap_or(bytes.len());
                if end > i {
                    let chunk = &src[i..end];
                    if chunk.trim().is_empty() {
                        b.push(chunk);
                    } else {
                        b.push_color(chunk, p.text());
                    }
                    i = end;
                }
                if i < bytes.len() && bytes[i] == b'&' {
                    let end = src[i..].find(';').map(|n| i + n + 1).unwrap_or(bytes.len());
                    b.push_color(&src[i..end], p.entity());
                    i = end;
                }
                continue;
            }
            let tag_start = i;
            i += 1;
            if i < bytes.len() && bytes[i] == b'/' {
                b.push_color("</", p.punct());
                i += 1;
            } else if src[i..].starts_with('?') {
                b.push_color("<?", p.punct());
                i += 1;
            } else {
                b.push_color("<", p.punct());
            }
            let name_start = i;
            while i < bytes.len() && !matches!(bytes[i], b' ' | b'\t' | b'\r' | b'\n' | b'/' | b'>' | b'?') {
                i += 1;
            }
            if i > name_start {
                b.push_color(&src[name_start..i], p.tag());
            }
            while i < bytes.len() {
                if bytes[i] == b'>' || bytes[i] == b'?' {
                    b.push_color(&src[i..i + 1], p.punct());
                    i += 1;
                    break;
                }
                if bytes[i].is_ascii_whitespace() {
                    let start = i;
                    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                        i += 1;
                    }
                    b.push(&src[start..i]);
                    continue;
                }
                if bytes[i] == b'/' {
                    b.push_color("/", p.punct());
                    i += 1;
                    continue;
                }
                let key_start = i;
                while i < bytes.len() && bytes[i] != b'=' && !bytes[i].is_ascii_whitespace() && bytes[i] != b'>' {
                    i += 1;
                }
                b.push_color(&src[key_start..i], p.attr());
                while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                    let start = i;
                    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                        i += 1;
                    }
                    b.push(&src[start..i]);
                }
                if i < bytes.len() && bytes[i] == b'=' {
                    b.push_color("=", p.punct());
                    i += 1;
                    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                        let start = i;
                        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                            i += 1;
                        }
                        b.push(&src[start..i]);
                    }
                }
                if i < bytes.len() && (bytes[i] == b'"' || bytes[i] == b'\'') {
                    let quote = bytes[i];
                    let start = i;
                    i += 1;
                    while i < bytes.len() && bytes[i] != quote {
                        i += 1;
                    }
                    if i < bytes.len() {
                        i += 1;
                    }
                    let raw = &src[start..i];
                    let inner = &raw[1..raw.len().saturating_sub(1)];
                    b.push_color(raw[0..1].to_owned().as_str(), p.punct());
                    Self::color_value(&mut b, inner, p);
                    if raw.len() > 1 {
                        b.push_color(&raw[raw.len() - 1..], p.punct());
                    }
                }
                if i == tag_start {
                    i += 1;
                }
            }
        }
        let _ = SpanStyle::default();
        TransformedText::new(b.build(), Box::new(IdentityOffsetMapping))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlight_keeps_exact_text() {
        let src = "<vector android:fillColor=\"#FF00FF00\">\n  <path android:pathData=\"M0,0\" />\n</vector>";
        let out = XmlHighlighter::new(Palette::Light).filter(&AnnotatedString::from(src));
        assert_eq!(out.text.as_str(), src);
    }
}
