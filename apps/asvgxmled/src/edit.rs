use std::ops::Range;

fn value_range(src: &str, range: &Range<usize>, attr: &str) -> Option<Range<usize>> {
    let chunk = src.get(range.clone())?;
    let needles = [format!("android:{attr}=\""), format!("{attr}=\"")];
    let found = needles.iter().find_map(|n| chunk.find(n).map(|i| (i, n.len())))?;
    let start = range.start + found.0 + found.1;
    let end = start + chunk[found.0 + found.1..].find('"')?;
    Some(start..end)
}

pub fn set_attr(src: &str, range: &Range<usize>, attr: &str, value: &str) -> String {
    let escaped = value.replace('"', "&quot;");
    if let Some(at) = value_range(src, range, attr) {
        let mut out = String::with_capacity(src.len() + escaped.len());
        out.push_str(&src[..at.start]);
        out.push_str(&escaped);
        out.push_str(&src[at.end..]);
        return out;
    }
    let Some(chunk) = src.get(range.clone()) else { return src.to_owned() };
    let Some(rel_end) = chunk.find('>') else { return src.to_owned() };
    let insert_at = range.start + rel_end;
    let slash = insert_at > 0 && src.as_bytes()[insert_at - 1] == b'/';
    let at = if slash { insert_at - 1 } else { insert_at };
    let mut out = String::with_capacity(src.len() + escaped.len() + attr.len() + 24);
    out.push_str(&src[..at]);
    out.push_str(&format!(" android:{attr}=\"{escaped}\""));
    if slash {
        out.push_str("/>");
        out.push_str(&src[insert_at + 1..]);
    } else {
        out.push_str(&src[insert_at..]);
    }
    out
}

pub fn add_root_wrapper(src: &str, attrs: &str) -> String {
    let Some(open) = src.find("<vector") else { return src.to_owned() };
    let Some(tag_end) = src[open..].find('>').map(|i| open + i) else { return src.to_owned() };
    let Some(root_end) = src.rfind("</vector>") else { return src.to_owned() };
    let mut out = String::with_capacity(src.len() + attrs.len() + 32);
    out.push_str(&src[..tag_end + 1]);
    out.push_str("\n    <group");
    out.push_str(attrs);
    out.push_str(">");
    out.push_str(&src[tag_end + 1..root_end]);
    out.push_str("    </group>\n");
    out.push_str(&src[root_end..]);
    out
}
