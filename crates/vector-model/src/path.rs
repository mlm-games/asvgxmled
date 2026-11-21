use kurbo::{Arc, BezPath, PathEl, Point, Vec2};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FillType {
    NonZero,
    EvenOdd,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PathError {
    pub message: String,
    pub offset: usize,
}

type PResult<T> = Result<T, PathError>;

pub fn parse_path_data(s: &str) -> PResult<BezPath> {
    let mut p = PathParser { src: s.as_bytes(), pos: 0, path: BezPath::new(), last_ctrl: None, last_cmd: None, start: Point::ORIGIN, current: Point::ORIGIN };
    p.run()?;
    Ok(p.path)
}

struct PathParser<'a> {
    src: &'a [u8],
    pos: usize,
    path: BezPath,
    last_ctrl: Option<Point>,
    last_cmd: Option<u8>,
    start: Point,
    current: Point,
}

impl PathParser<'_> {
    fn run(&mut self) -> PResult<()> {
        loop {
            self.skip_separators();
            if self.pos >= self.src.len() {
                return Ok(());
            }
            let cmd = self.src[self.pos];
            if !cmd.is_ascii_alphabetic() {
                return self.err(format!("unexpected character '{}' where a path command was expected", cmd as char));
            }
            self.pos += 1;
            let rel = cmd.is_ascii_lowercase();
            let upper = cmd.to_ascii_uppercase();
            match upper {
                b'Z' => {
                    self.path.close_path();
                    self.current = self.start;
                    self.last_ctrl = None;
                    self.last_cmd = Some(upper);
                }
                b'M' | b'L' | b'T' => {
                    let mut first = true;
                    loop {
                        let x = self.number()?;
                        let y = self.number()?;
                        let to = if rel { self.current + Vec2::new(x, y) } else { Point::new(x, y) };
                        match upper {
                            b'M' => {
                                if first {
                                    self.path.move_to(to);
                                    self.start = to;
                                } else {
                                    self.path.line_to(to);
                                }
                                self.last_ctrl = None;
                            }
                            b'L' => {
                                self.path.line_to(to);
                                self.last_ctrl = None;
                            }
                            b'T' => {
                                let ctrl = self.last_ctrl.map(|c| self.current + (self.current - c)).unwrap_or(self.current);
                                self.path.quad_to(ctrl, to);
                                self.last_ctrl = Some(ctrl);
                            }
                            _ => unreachable!(),
                        }
                        self.current = to;
                        first = false;
                        if !self.has_number() {
                            break;
                        }
                    }
                    self.last_cmd = Some(upper);
                }
                b'H' | b'V' => {
                    loop {
                        let v = self.number()?;
                        let to = if upper == b'H' {
                            Point::new(if rel { self.current.x + v } else { v }, self.current.y)
                        } else {
                            Point::new(self.current.x, if rel { self.current.y + v } else { v })
                        };
                        self.path.line_to(to);
                        self.current = to;
                        self.last_ctrl = None;
                        if !self.has_number() {
                            break;
                        }
                    }
                    self.last_cmd = Some(upper);
                }
                b'C' => {
                    loop {
                        let c1 = self.point(rel)?;
                        let c2 = self.point(rel)?;
                        let to = self.point(rel)?;
                        self.path.curve_to(c1, c2, to);
                        self.current = to;
                        self.last_ctrl = Some(c2);
                        if !self.has_number() {
                            break;
                        }
                    }
                    self.last_cmd = Some(upper);
                }
                b'S' => {
                    loop {
                        let reflected = if matches!(self.last_cmd, Some(b'C') | Some(b'S')) {
                            self.last_ctrl.map(|c| self.current + (self.current - c))
                        } else {
                            None
                        };
                        let c1 = reflected.unwrap_or(self.current);
                        let c2 = self.point(rel)?;
                        let to = self.point(rel)?;
                        self.path.curve_to(c1, c2, to);
                        self.current = to;
                        self.last_ctrl = Some(c2);
                        if !self.has_number() {
                            break;
                        }
                    }
                    self.last_cmd = Some(upper);
                }
                b'Q' => {
                    loop {
                        let c = self.point(rel)?;
                        let to = self.point(rel)?;
                        self.path.quad_to(c, to);
                        self.current = to;
                        self.last_ctrl = Some(c);
                        if !self.has_number() {
                            break;
                        }
                    }
                    self.last_cmd = Some(upper);
                }
                b'A' => {
                    loop {
                        let rx = self.number()?;
                        let ry = self.number()?;
                        let rotation = self.number()?;
                        let large = self.flag()?;
                        let sweep = self.flag()?;
                        let to = self.point(rel)?;
                        self.arc_to(rx, ry, rotation, large, sweep, to)?;
                        self.current = to;
                        self.last_ctrl = None;
                        if !self.has_number() {
                            break;
                        }
                    }
                    self.last_cmd = Some(upper);
                }
                _ => return self.err(format!("unsupported path command '{}'", cmd as char)),
            }
        }
    }

    fn arc_to(&mut self, rx: f64, ry: f64, rotation_deg: f64, large: bool, sweep: bool, to: Point) -> PResult<()> {
        if self.current == to {
            return Ok(());
        }
        if rx == 0.0 || ry == 0.0 {
            self.path.line_to(to);
            return Ok(());
        }
        let phi = rotation_deg.to_radians();
        let (cos_phi, sin_phi) = (phi.cos(), phi.sin());
        let dx = self.current.x - to.x;
        let dy = self.current.y - to.y;
        let x1 = cos_phi * dx / 2.0 + sin_phi * dy / 2.0;
        let y1 = -sin_phi * dx / 2.0 + cos_phi * dy / 2.0;
        let mut rx = rx.abs();
        let mut ry = ry.abs();
        let lambda = x1 * x1 / (rx * rx) + y1 * y1 / (ry * ry);
        if lambda > 1.0 {
            let s = lambda.sqrt();
            rx *= s;
            ry *= s;
        }
        let num = rx * rx * ry * ry - rx * rx * y1 * y1 - ry * ry * x1 * x1;
        let den = rx * rx * y1 * y1 + ry * ry * x1 * x1;
        let coef = if large == sweep { -1.0 } else { 1.0 } * (num / den).max(0.0).sqrt();
        let cx1 = coef * rx * y1 / ry;
        let cy1 = coef * -ry * x1 / rx;
        let cx = cos_phi * cx1 - sin_phi * cy1 + (self.current.x + to.x) / 2.0;
        let cy = sin_phi * cx1 + cos_phi * cy1 + (self.current.y + to.y) / 2.0;
        let ux = (x1 - cx1) / rx;
        let uy = (y1 - cy1) / ry;
        let vx = (-x1 - cx1) / rx;
        let vy = (-y1 - cy1) / ry;
        let theta1 = uy.atan2(ux);
        let mut delta = (ux * vy - uy * vx).atan2(ux * vx + uy * vy);
        if !sweep && delta > 0.0 {
            delta -= std::f64::consts::TAU;
        } else if sweep && delta < 0.0 {
            delta += std::f64::consts::TAU;
        }
        let arc = Arc::new(Point::new(cx, cy), Vec2::new(rx, ry), theta1, delta, phi);
        let mut iter = arc.append_iter(0.05);
        if let Some(el) = iter.next() {
            match el {
                PathEl::CurveTo(_, _, _) => self.path.line_to(arc_start(arc)),
                _ => {}
            }
            self.path.push(el);
        }
        for el in iter {
            self.path.push(el);
        }
        Ok(())
    }

    fn point(&mut self, rel: bool) -> PResult<Point> {
        let x = self.number()?;
        let y = self.number()?;
        Ok(if rel { self.current + Vec2::new(x, y) } else { Point::new(x, y) })
    }

    fn flag(&mut self) -> PResult<bool> {
        self.skip_separators();
        match self.src.get(self.pos) {
            Some(b'0') => { self.pos += 1; Ok(false) }
            Some(b'1') => { self.pos += 1; Ok(true) }
            _ => self.err("arc flag must be 0 or 1".into()),
        }
    }

    fn has_number(&mut self) -> bool {
        let save = self.pos;
        self.skip_separators();
        let ok = self.pos < self.src.len()
            && (self.src[self.pos].is_ascii_digit()
                || self.src[self.pos] == b'.'
                || self.src[self.pos] == b'+'
                || self.src[self.pos] == b'-');
        if !ok {
            self.pos = save;
        }
        ok
    }

    fn skip_separators(&mut self) {
        while let Some(&b) = self.src.get(self.pos) {
            if matches!(b, b' ' | b'\t' | b'\r' | b'\n' | b',') {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    fn number(&mut self) -> PResult<f64> {
        self.skip_separators();
        let start = self.pos;
        if matches!(self.src.get(self.pos), Some(b'+') | Some(b'-')) {
            self.pos += 1;
        }
        let mut digits = 0;
        while matches!(self.src.get(self.pos), Some(b) if b.is_ascii_digit()) {
            self.pos += 1;
            digits += 1;
        }
        if self.src.get(self.pos) == Some(&b'.') {
            self.pos += 1;
            while matches!(self.src.get(self.pos), Some(b) if b.is_ascii_digit()) {
                self.pos += 1;
                digits += 1;
            }
        }
        if digits == 0 {
            return self.err("expected a number".into());
        }
        if matches!(self.src.get(self.pos), Some(b'e') | Some(b'E')) {
            let exp_start = self.pos;
            self.pos += 1;
            if matches!(self.src.get(self.pos), Some(b'+') | Some(b'-')) {
                self.pos += 1;
            }
            let mut exp_digits = 0;
            while matches!(self.src.get(self.pos), Some(b) if b.is_ascii_digit()) {
                self.pos += 1;
                exp_digits += 1;
            }
            if exp_digits == 0 {
                self.pos = exp_start;
            }
        }
        let text = std::str::from_utf8(&self.src[start..self.pos]).map_err(|_| PathError { message: "invalid number".into(), offset: start })?;
        text.parse::<f64>().map_err(|_| PathError { message: format!("invalid number '{text}'"), offset: start })
    }

    fn err<T>(&self, message: String) -> PResult<T> {
        Err(PathError { message, offset: self.pos })
    }
}

fn arc_start(arc: Arc) -> Point {
    let px = arc.radii.x * arc.start_angle.cos();
    let py = arc.radii.y * arc.start_angle.sin();
    arc.center
        + Vec2::new(
            px * arc.x_rotation.cos() - py * arc.x_rotation.sin(),
            px * arc.x_rotation.sin() + py * arc.x_rotation.cos(),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_relative_lines_and_close() {
        let p = parse_path_data("M10 10 l10 0 0 10 z").unwrap();
        assert_eq!(p.elements().len(), 4);
    }

    #[test]
    fn parses_arc() {
        let p = parse_path_data("M0 0 A 10 10 0 0 1 20 0").unwrap();
        assert!(p.elements().len() > 2);
    }
}
