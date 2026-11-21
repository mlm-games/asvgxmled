#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba {
    pub const TRANSPARENT: Self = Self { r: 0, g: 0, b: 0, a: 0 };
    pub const BLACK: Self = Self { r: 0, g: 0, b: 0, a: 255 };
    pub const WHITE: Self = Self { r: 255, g: 255, b: 255, a: 255 };

    pub fn parse(s: &str) -> Option<Self> {
        let h = s.strip_prefix('#')?;
        let nib = |i: usize| u8::from_str_radix(&h[i..i + 1], 16).ok();
        match h.len() {
            3 => Some(Self { r: nib(0)? * 17, g: nib(1)? * 17, b: nib(2)? * 17, a: 255 }),
            4 => Some(Self { a: nib(0)? * 17, r: nib(1)? * 17, g: nib(2)? * 17, b: nib(3)? * 17 }),
            6 => Some(Self {
                r: u8::from_str_radix(&h[0..2], 16).ok()?,
                g: u8::from_str_radix(&h[2..4], 16).ok()?,
                b: u8::from_str_radix(&h[4..6], 16).ok()?,
                a: 255,
            }),
            8 => Some(Self {
                a: u8::from_str_radix(&h[0..2], 16).ok()?,
                r: u8::from_str_radix(&h[2..4], 16).ok()?,
                g: u8::from_str_radix(&h[4..6], 16).ok()?,
                b: u8::from_str_radix(&h[6..8], 16).ok()?,
            }),
            _ => None,
        }
    }

    pub fn to_argb_hex(self) -> String {
        if self.a == 255 {
            format!("#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
        } else {
            format!("#{:02X}{:02X}{:02X}{:02X}", self.a, self.r, self.g, self.b)
        }
    }

    pub fn with_alpha(self, alpha: u8) -> Self {
        Self { a: alpha, ..self }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ColorSpec {
    Literal(Rgba),
    Reference(String),
}

impl ColorSpec {
    pub fn parse(s: &str) -> Option<Self> {
        let t = s.trim();
        if let Some(hex) = Rgba::parse(t) {
            return Some(Self::Literal(hex));
        }
        if t.starts_with('?') || t.starts_with("@color/") || t.starts_with("@android:color/") {
            return Some(Self::Reference(t.to_owned()));
        }
        None
    }

    pub fn is_reference(&self) -> bool {
        matches!(self, Self::Reference(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn android_alpha_comes_first() {
        assert_eq!(Rgba::parse("#80FF0000"), Some(Rgba { r: 255, g: 0, b: 0, a: 128 }));
        assert_eq!(Rgba::parse("#F00"), Some(Rgba { r: 255, g: 0, b: 0, a: 255 }));
        assert_eq!(Rgba::parse("#8F00"), Some(Rgba { r: 255, g: 0, b: 0, a: 136 }));
    }
}
