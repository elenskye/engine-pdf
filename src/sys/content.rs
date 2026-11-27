//! 页面内容流：照 PDFium 列对象的规则，数出每个页面对象由哪条指令画出，只改这几条指令。
//!
//! PDFium 重写整页会丢掉字距微调，偶尔丢掉整段内容（`decisions/0019`）。所以 PDFium 只负责列对象、
//! 取字框、解图，改动由 lopdf 写在原指令上，其余指令原样保留。对不上 PDFium 的对象清单、
//! 或含 lopdf 解析不全的内联图片时，整页不改。

use std::collections::{BTreeMap, HashSet};

use lopdf::content::{Content, Operation};
use lopdf::{Dictionary, Document, Object, ObjectId, Stream};

use crate::Error;
use crate::edit::Rect;

use super::objects::{self, value};

/// PDFium 的页面对象种类。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Kind {
    Text,
    Image,
    Path,
    Form,
    Shading,
}

/// 指令改不了：需要的字宽或矩阵拿不到。调用方拒绝编辑或改走整页位图。
#[derive(Debug)]
pub(super) struct Unsupported;

type Matrix = [f64; 6];
const IDENTITY: Matrix = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

#[derive(Clone)]
struct Graphics {
    ctm: Matrix,
    font: Option<(Vec<u8>, f64)>,
    char_spacing: f64,
    word_spacing: f64,
    scale: f64,
    leading: f64,
    rise: f64,
    /// 填充或描边用的是图案（图案里可以藏图片）。
    fill_pattern: bool,
    stroke_pattern: bool,
}

impl Default for Graphics {
    fn default() -> Self {
        Self {
            ctm: IDENTITY,
            font: None,
            char_spacing: 0.0,
            word_spacing: 0.0,
            scale: 1.0,
            leading: 0.0,
            rise: 0.0,
            fill_pattern: false,
            stroke_pattern: false,
        }
    }
}

struct Item {
    kind: Kind,
    op: usize,
    graphics: Graphics,
    /// 文字的行矩阵（`'`、`"` 已换过行）。
    line: Matrix,
    /// 同一个文字对象里，后面还有接着这一段往右画的字（中间没有换行或定位）。
    continued: bool,
    /// 外层的 `BDC` / `BMC` 指令。
    marks: Vec<usize>,
    /// 图片、Form 在页面上占的范围。
    bounds: Option<Rect>,
    /// 路径的端点和控制点（页面坐标）。
    vertices: Vec<(f64, f64)>,
}

enum Edit {
    Delete,
    Move(f64, f64),
    Image(Vec<u8>),
}

pub(super) struct PageContent {
    ops: Vec<Operation>,
    items: Vec<Item>,
    resources: Dictionary,
    edits: BTreeMap<usize, Edit>,
    marks: BTreeMap<usize, Operation>,
    images: Vec<(Vec<u8>, ObjectId)>,
    cover: Vec<Rect>,
}

impl PageContent {
    /// 读一页的内容流。读不了、或含内联图片时返回 `None`。
    pub(super) fn parse(doc: &Document, page: ObjectId) -> Option<Self> {
        let dict = doc.get_dictionary(page).ok()?;
        let resources = objects::inherited(doc, dict, b"Resources")
            .and_then(|resources| resources.as_dict().ok())
            .cloned()
            .unwrap_or_default();
        let mut bytes = Vec::new();
        for id in doc.get_page_contents(page) {
            let stream = doc.get_object(id).ok()?.as_stream().ok()?;
            bytes.extend(stream.get_plain_content().ok()?);
            bytes.push(b'\n');
        }
        let ops = Content::decode(&bytes).ok()?.operations;
        // lopdf 跳过它解不了的内联图片，原样写回会丢图。
        if ops.iter().any(|op| op.operator == "BI") {
            return None;
        }
        let items = walk(doc, &resources, &ops);
        Some(Self {
            ops,
            items,
            resources,
            edits: BTreeMap::new(),
            marks: BTreeMap::new(),
            images: Vec::new(),
            cover: Vec::new(),
        })
    }

    /// 和 PDFium 的对象清单逐个对上：种类相同，图片位置相同。
    pub(super) fn matches(&self, objects: &[(Option<Kind>, Rect)]) -> bool {
        self.items.len() == objects.len()
            && self
                .items
                .iter()
                .zip(objects)
                .all(|(item, (kind, bounds))| {
                    Some(item.kind) == *kind
                        && (item.kind != Kind::Image
                            || item.bounds.is_some_and(|own| close(own, *bounds)))
                })
    }

    /// 路径压着涂黑框，但所有端点和控制点都在框外、也不用图案上色：框里那一段完全由框外的点决定，
    /// 文件里不留框里独有的东西，盖上黑框即可。
    pub(super) fn path_clear_of(&self, index: usize, areas: &[Rect]) -> bool {
        let item = &self.items[index];
        item.kind == Kind::Path
            && !item.graphics.fill_pattern
            && !item.graphics.stroke_pattern
            && item.vertices.iter().all(|&(x, y)| {
                x.is_finite()
                    && y.is_finite()
                    && !areas
                        .iter()
                        .any(|a| a.x <= x && x <= a.x + a.width && a.y <= y && y <= a.y + a.height)
            })
    }

    /// 一个图片对象的 XObject 字典。
    pub(super) fn image<'a>(&'a self, doc: &'a Document, index: usize) -> Option<&'a Dictionary> {
        let name = self.ops[self.items[index].op]
            .operands
            .first()?
            .as_name()
            .ok()?;
        let all = value(doc, &self.resources, b"XObject")?.as_dict().ok()?;
        Some(&value(doc, all, name)?.as_stream().ok()?.dict)
    }

    pub(super) fn delete(&mut self, doc: &Document, index: usize) -> Result<(), Unsupported> {
        let item = &self.items[index];
        if item.kind == Kind::Text && item.continued {
            // 后面的字接着这一段画，删掉后要补上同样的前进量。
            self.advance(doc, item).ok_or(Unsupported)?;
        }
        self.edits.insert(item.op, Edit::Delete);
        Ok(())
    }

    /// 在页面坐标里平移一个文字或图片对象，可多次累加。
    pub(super) fn translate(
        &mut self,
        doc: &Document,
        index: usize,
        dx: f64,
        dy: f64,
    ) -> Result<(), Unsupported> {
        let item = &self.items[index];
        let (mut total_x, mut total_y) = match self.edits.get(&item.op) {
            Some(Edit::Move(x, y)) => (*x, *y),
            Some(_) => return Err(Unsupported),
            None => (0.0, 0.0),
        };
        total_x += dx;
        total_y += dy;
        match item.kind {
            Kind::Image | Kind::Form => {
                local_delta(item.graphics.ctm, total_x, total_y).ok_or(Unsupported)?;
            }
            Kind::Text => {
                let (_, size) = item.graphics.font.as_ref().ok_or(Unsupported)?;
                if size * item.graphics.scale == 0.0 || self.vertical(doc, item) {
                    return Err(Unsupported);
                }
                local_delta(multiply(item.line, item.graphics.ctm), total_x, total_y)
                    .ok_or(Unsupported)?;
            }
            Kind::Path | Kind::Shading => return Err(Unsupported),
        }
        self.edits.insert(item.op, Edit::Move(total_x, total_y));
        Ok(())
    }

    /// 换成另一张图（`name` 是新加的页面资源名）。
    pub(super) fn replace_image(&mut self, index: usize, image: ObjectId) {
        let name = format!("EnginePDFRedacted{}", self.images.len() + 1).into_bytes();
        self.images.push((name.clone(), image));
        self.edits.insert(self.items[index].op, Edit::Image(name));
    }

    /// 去掉外层标记内容上的替代文字，免得删掉的字留在 `ActualText` 里。
    pub(super) fn strip_alternates(&mut self, doc: &Document, index: usize) {
        for &mark in &self.items[index].marks {
            let op = self.marks.get(&mark).unwrap_or(&self.ops[mark]);
            let Some(properties) = op.operands.get(1) else {
                continue;
            };
            let properties = match properties {
                Object::Name(name) => value(doc, &self.resources, b"Properties")
                    .and_then(|all| all.as_dict().ok())
                    .and_then(|all| value(doc, all, name))
                    .and_then(|properties| properties.as_dict().ok()),
                other => other.as_dict().ok(),
            };
            let Some(properties) = properties else {
                continue;
            };
            if ALTERNATES.iter().any(|key| properties.has(key)) {
                let mut properties = properties.clone();
                for key in ALTERNATES {
                    properties.remove(key);
                }
                let mut op = op.clone();
                op.operands[1] = Object::Dictionary(properties);
                self.marks.insert(mark, op);
            }
        }
    }

    /// 在页面最上层盖黑框（页面坐标）。
    pub(super) fn cover(&mut self, areas: &[Rect]) {
        self.cover.extend_from_slice(areas);
    }

    /// 写回页面：新内容流，用到的图片资源放进页面自己的资源字典。
    pub(super) fn write(&self, doc: &mut Document, page: ObjectId) -> Result<(), Error> {
        let mut ops = Vec::with_capacity(self.ops.len() + 8);
        if !self.cover.is_empty() {
            ops.push(Operation::new("q", vec![]));
        }
        let mut depth = 0_usize;
        for (index, op) in self.ops.iter().enumerate() {
            match op.operator.as_str() {
                "q" => depth += 1,
                "Q" => depth = depth.saturating_sub(1),
                _ => {}
            }
            let op = self.marks.get(&index).unwrap_or(op);
            match self.edits.get(&index) {
                None => ops.push(op.clone()),
                Some(edit) => {
                    let item = self
                        .items
                        .iter()
                        .find(|item| item.op == index)
                        .ok_or(Error::SaveFailed)?;
                    ops.extend(self.rewrite(doc, item, edit).ok_or(Error::SaveFailed)?);
                }
            }
        }
        if !self.cover.is_empty() {
            ops.extend((0..=depth).map(|_| Operation::new("Q", vec![])));
            ops.push(Operation::new("q", vec![]));
            ops.push(Operation::new("g", vec![0.into()]));
            for area in &self.cover {
                let rect = [area.x, area.y, area.width, area.height].map(real);
                ops.push(Operation::new("re", rect.into()));
            }
            ops.push(Operation::new("f", vec![]));
            ops.push(Operation::new("Q", vec![]));
        }
        let used: HashSet<Vec<u8>> = ops
            .iter()
            .filter(|op| op.operator == "Do")
            .filter_map(|op| op.operands.first()?.as_name().ok().map(<[u8]>::to_vec))
            .collect();
        let encoded = Content { operations: ops }
            .encode()
            .map_err(|_| Error::SaveFailed)?;
        let mut stream = Stream::new(Dictionary::new(), encoded);
        stream.compress().map_err(|_| Error::SaveFailed)?;
        let contents = doc.add_object(stream);

        let mut resources = self.resources.clone();
        let mut xobjects = value(doc, &resources, b"XObject")
            .and_then(|xobjects| xobjects.as_dict().ok())
            .cloned()
            .unwrap_or_default();
        for (name, id) in &self.images {
            xobjects.set(name.clone(), *id);
        }
        // 删掉或换掉的图不再挂在这一页上，没有别处用到时随存盘清掉。
        let unused: Vec<_> = xobjects
            .iter()
            .map(|(name, _)| name.clone())
            .filter(|name| !used.contains(name))
            .collect();
        for name in unused {
            xobjects.remove(&name);
        }
        if xobjects.is_empty() {
            resources.remove(b"XObject");
        } else {
            resources.set("XObject", xobjects);
        }
        let page = doc
            .get_dictionary_mut(page)
            .map_err(|_| Error::SaveFailed)?;
        page.set("Contents", contents);
        page.set("Resources", resources);
        Ok(())
    }

    fn rewrite(&self, doc: &Document, item: &Item, edit: &Edit) -> Option<Vec<Operation>> {
        let op = &self.ops[item.op];
        let mut ops = Vec::new();
        match (item.kind, edit) {
            (Kind::Path, Edit::Delete) => ops.push(Operation::new("n", vec![])),
            (Kind::Text, Edit::Delete) => {
                ops.extend(line_break(op));
                if item.continued {
                    let advance = self.advance(doc, item)?;
                    ops.extend(shift(item, advance));
                }
            }
            (_, Edit::Delete) => {}
            (Kind::Image | Kind::Form, Edit::Move(dx, dy)) => {
                let (x, y) = local_delta(item.graphics.ctm, *dx, *dy)?;
                ops.push(Operation::new("q", vec![]));
                let matrix = [1.0, 0.0, 0.0, 1.0, x, y].map(real);
                ops.push(Operation::new("cm", matrix.into()));
                ops.push(op.clone());
                ops.push(Operation::new("Q", vec![]));
            }
            (Kind::Text, Edit::Move(dx, dy)) => {
                let (x, y) = local_delta(multiply(item.line, item.graphics.ctm), *dx, *dy)?;
                let rise = item.graphics.rise;
                ops.extend(line_break(op));
                ops.extend(shift(item, x));
                if y != 0.0 {
                    ops.push(Operation::new("Ts", vec![real(rise + y)]));
                }
                ops.push(match op.operator.as_str() {
                    "'" => Operation::new("Tj", vec![op.operands.first()?.clone()]),
                    "\"" => Operation::new("Tj", vec![op.operands.get(2)?.clone()]),
                    _ => op.clone(),
                });
                if y != 0.0 {
                    ops.push(Operation::new("Ts", vec![real(rise)]));
                }
                ops.extend(shift(item, -x));
            }
            (_, Edit::Image(name)) => {
                ops.push(Operation::new("Do", vec![Object::Name(name.clone())]));
            }
            _ => return None,
        }
        Some(ops)
    }

    /// 这一段字在文字空间里向右前进多少（未乘文字矩阵）。
    fn advance(&self, doc: &Document, item: &Item) -> Option<f64> {
        let g = &item.graphics;
        let (name, size) = g.font.as_ref()?;
        let font = self.font(doc, name)?;
        let metrics = Metrics::read(doc, font)?;
        let op = &self.ops[item.op];
        let parts: Vec<&Object> = match op.operator.as_str() {
            "TJ" => op.operands.first()?.as_array().ok()?.iter().collect(),
            "\"" => vec![op.operands.get(2)?],
            _ => vec![op.operands.first()?],
        };
        let mut advance = 0.0;
        for part in parts {
            match part {
                Object::String(bytes, _) => {
                    for (code, single) in metrics.codes(bytes)? {
                        let spacing = if single && code == 32 {
                            g.char_spacing + g.word_spacing
                        } else {
                            g.char_spacing
                        };
                        advance += (metrics.width(code) * size + spacing) * g.scale;
                    }
                }
                other => advance -= number(other)? / 1000.0 * size * g.scale,
            }
        }
        Some(advance)
    }

    fn font<'a>(&'a self, doc: &'a Document, name: &[u8]) -> Option<&'a Dictionary> {
        let fonts = value(doc, &self.resources, b"Font")?.as_dict().ok()?;
        value(doc, fonts, name)?.as_dict().ok()
    }

    fn vertical(&self, doc: &Document, item: &Item) -> bool {
        let Some((name, _)) = &item.graphics.font else {
            return true;
        };
        self.font(doc, name)
            .and_then(|font| value(doc, font, b"Encoding"))
            .and_then(|encoding| encoding.as_name().ok())
            .is_some_and(|encoding| encoding.ends_with(b"-V"))
    }
}

const ALTERNATES: [&[u8]; 3] = [b"ActualText", b"Alt", b"E"];

/// `'`、`"` 先换行再画字，删掉或挪走字时保留换行（和 `"` 设的间距）。
fn line_break(op: &Operation) -> Vec<Operation> {
    match op.operator.as_str() {
        "'" => vec![Operation::new("T*", vec![])],
        "\"" => vec![
            Operation::new("Tw", vec![op.operands[0].clone()]),
            Operation::new("Tc", vec![op.operands[1].clone()]),
            Operation::new("T*", vec![]),
        ],
        _ => vec![],
    }
}

/// 沿文字方向挪 `x`（文字空间单位），用只有数字的 `TJ`：不碰行矩阵，后面接着画的字照旧。
fn shift(item: &Item, x: f64) -> Option<Operation> {
    if x == 0.0 {
        return None;
    }
    let (_, size) = item.graphics.font.as_ref()?;
    let adjust = -x * 1000.0 / (size * item.graphics.scale);
    adjust
        .is_finite()
        .then(|| Operation::new("TJ", vec![Object::Array(vec![real(adjust)])]))
}

fn walk(doc: &Document, resources: &Dictionary, ops: &[Operation]) -> Vec<Item> {
    let mut items: Vec<Item> = Vec::new();
    let mut stack = Vec::new();
    let mut g = Graphics::default();
    let mut line = IDENTITY;
    let mut points = 0_usize;
    let mut last_move = false;
    let mut vertices = Vec::new();
    let mut marks = Vec::new();
    // 上一段画出对象的字：后面的字若不换行就接着它画。
    let mut pending: Option<usize> = None;
    let xobject = |name: &[u8]| {
        let all = value(doc, resources, b"XObject")?.as_dict().ok()?;
        let stream = value(doc, all, name)?.as_stream().ok()?;
        value(doc, &stream.dict, b"Subtype")?.as_name().ok()
    };
    for (index, op) in ops.iter().enumerate() {
        let args = &op.operands;
        let n = |i: usize| args.get(i).and_then(number);
        let mut created = None;
        match op.operator.as_str() {
            "q" => stack.push(g.clone()),
            "Q" => {
                if let Some(saved) = stack.pop() {
                    g = saved;
                }
            }
            "cm" => {
                if let Some(m) = matrix(args) {
                    g.ctm = multiply(m, g.ctm);
                }
            }
            "BT" | "ET" => {
                line = IDENTITY;
                pending = None;
            }
            "Td" | "TD" => {
                if let (Some(x), Some(y)) = (n(0), n(1)) {
                    if op.operator == "TD" {
                        g.leading = -y;
                    }
                    line = multiply([1.0, 0.0, 0.0, 1.0, x, y], line);
                }
                pending = None;
            }
            "Tm" => {
                if let Some(m) = matrix(args) {
                    line = m;
                }
                pending = None;
            }
            "T*" => {
                line = multiply([1.0, 0.0, 0.0, 1.0, 0.0, -g.leading], line);
                pending = None;
            }
            "Tf" => {
                if let (Some(name), Some(size)) =
                    (args.first().and_then(|a| a.as_name().ok()), n(1))
                {
                    g.font = Some((name.to_vec(), size));
                }
            }
            "Tc" => g.char_spacing = n(0).unwrap_or(g.char_spacing),
            "Tw" => g.word_spacing = n(0).unwrap_or(g.word_spacing),
            "Tz" => g.scale = n(0).map_or(g.scale, |v| v / 100.0),
            "TL" => g.leading = n(0).unwrap_or(g.leading),
            "Ts" => g.rise = n(0).unwrap_or(g.rise),
            "Tj" | "TJ" | "'" | "\"" => {
                if matches!(op.operator.as_str(), "'" | "\"") {
                    if op.operator == "\"" {
                        g.word_spacing = n(0).unwrap_or(g.word_spacing);
                        g.char_spacing = n(1).unwrap_or(g.char_spacing);
                    }
                    line = multiply([1.0, 0.0, 0.0, 1.0, 0.0, -g.leading], line);
                    pending = None;
                }
                let text = match op.operator.as_str() {
                    "TJ" => args
                        .first()
                        .and_then(|a| a.as_array().ok())
                        .is_some_and(|parts| {
                            parts
                                .iter()
                                .any(|part| matches!(part, Object::String(s, _) if !s.is_empty()))
                        }),
                    "\"" => matches!(args.get(2), Some(Object::String(s, _)) if !s.is_empty()),
                    _ => matches!(args.first(), Some(Object::String(s, _)) if !s.is_empty()),
                };
                // 只挪位置、不画字的 TJ 不算接着画。
                if text {
                    if let Some(previous) = pending {
                        items[previous].continued = true;
                    }
                    pending = None;
                    if g.font.is_some() {
                        created = Some((Kind::Text, None));
                        pending = Some(items.len());
                    }
                }
            }
            "Do" => {
                let name = args.first().and_then(|a| a.as_name().ok());
                created = match name.and_then(xobject) {
                    Some(b"Image") => Some((Kind::Image, Some(unit_bounds(g.ctm)))),
                    Some(b"Form") => Some((Kind::Form, Some(unit_bounds(g.ctm)))),
                    _ => None,
                };
            }
            "sh" => {
                let name = args.first().and_then(|a| a.as_name().ok());
                let found = name.is_some_and(|name| {
                    value(doc, resources, b"Shading")
                        .and_then(|all| all.as_dict().ok())
                        .is_some_and(|all| value(doc, all, name).is_some())
                });
                if found {
                    created = Some((Kind::Shading, None));
                }
            }
            "m" => {
                if !(last_move && points > 0) {
                    points += 1;
                }
                last_move = true;
                vertices.extend(corners(args, g.ctm));
            }
            "l" | "c" | "v" | "y" | "re" => {
                points += match op.operator.as_str() {
                    "l" => 1,
                    "re" => 5,
                    _ => 3,
                };
                last_move = false;
                vertices.extend(if op.operator == "re" {
                    match (n(0), n(1), n(2), n(3)) {
                        (Some(x), Some(y), Some(w), Some(h)) => {
                            [(x, y), (x + w, y), (x, y + h), (x + w, y + h)]
                                .map(|(x, y)| transform(g.ctm, x, y))
                                .to_vec()
                        }
                        _ => vec![(f64::NAN, f64::NAN)],
                    }
                } else {
                    corners(args, g.ctm)
                });
            }
            "f" | "F" | "f*" | "S" | "s" | "B" | "B*" | "b" | "b*" => {
                if points >= 2 {
                    created = Some((Kind::Path, None));
                }
                points = 0;
                last_move = false;
            }
            "n" => {
                points = 0;
                last_move = false;
                vertices.clear();
            }
            "cs" | "CS" => {
                let pattern = args.first().and_then(|a| a.as_name().ok()) == Some(b"Pattern");
                if op.operator == "cs" {
                    g.fill_pattern = pattern;
                } else {
                    g.stroke_pattern = pattern;
                }
            }
            "scn" | "sc" | "g" | "rg" | "k" => {
                g.fill_pattern = matches!(args.last(), Some(Object::Name(_)));
            }
            "SCN" | "SC" | "G" | "RG" | "K" => {
                g.stroke_pattern = matches!(args.last(), Some(Object::Name(_)));
            }
            "BDC" | "BMC" => marks.push(index),
            "EMC" => {
                marks.pop();
            }
            _ => {}
        }
        if let Some((kind, bounds)) = created {
            items.push(Item {
                kind,
                op: index,
                graphics: g.clone(),
                line,
                continued: false,
                marks: marks.clone(),
                bounds,
                vertices: if kind == Kind::Path {
                    std::mem::take(&mut vertices)
                } else {
                    Vec::new()
                },
            });
        }
        if matches!(
            op.operator.as_str(),
            "f" | "F" | "f*" | "S" | "s" | "B" | "B*" | "b" | "b*"
        ) {
            vertices.clear();
        }
    }
    items
}

/// 指令操作数两两一组当作点，换到页面坐标。
fn corners(args: &[Object], ctm: Matrix) -> Vec<(f64, f64)> {
    args.chunks(2)
        .map(|pair| match pair {
            [x, y] => match (number(x), number(y)) {
                (Some(x), Some(y)) => transform(ctm, x, y),
                _ => (f64::NAN, f64::NAN),
            },
            _ => (f64::NAN, f64::NAN),
        })
        .collect()
}

fn transform(m: Matrix, x: f64, y: f64) -> (f64, f64) {
    (x * m[0] + y * m[2] + m[4], x * m[1] + y * m[3] + m[5])
}

/// 文字的字宽表。只认能确定字码和字宽的字体：带 `/Widths` 的单字节字体、`Identity-H` 的 CID 字体。
enum Metrics {
    Simple {
        first: i64,
        widths: Vec<f64>,
        missing: f64,
        scale: f64,
    },
    Identity {
        default: f64,
        ranges: Vec<(u32, u32, f64)>,
    },
}

impl Metrics {
    fn read(doc: &Document, font: &Dictionary) -> Option<Self> {
        let subtype = value(doc, font, b"Subtype")?.as_name().ok()?;
        if subtype == b"Type0" {
            if value(doc, font, b"Encoding")?.as_name().ok()? != b"Identity-H" {
                return None;
            }
            let descendant = value(doc, font, b"DescendantFonts")?.as_array().ok()?;
            let descendant = doc
                .dereference(descendant.first()?)
                .ok()?
                .1
                .as_dict()
                .ok()?;
            let default = value(doc, descendant, b"DW")
                .and_then(number)
                .unwrap_or(1000.0);
            let mut ranges = Vec::new();
            if let Some(list) = value(doc, descendant, b"W").and_then(|w| w.as_array().ok()) {
                let mut index = 0;
                while index + 1 < list.len() {
                    let first = number(&list[index])? as u32;
                    match doc.dereference(&list[index + 1]).ok()?.1 {
                        Object::Array(widths) => {
                            for (offset, width) in widths.iter().enumerate() {
                                let code = first + offset as u32;
                                ranges.push((code, code, number(width)? / 1000.0));
                            }
                            index += 2;
                        }
                        last => {
                            let width = number(list.get(index + 2)?)? / 1000.0;
                            ranges.push((first, number(last)? as u32, width));
                            index += 3;
                        }
                    }
                }
            }
            return Some(Self::Identity {
                default: default / 1000.0,
                ranges,
            });
        }
        let widths = value(doc, font, b"Widths")?
            .as_array()
            .ok()?
            .iter()
            .map(|width| doc.dereference(width).ok().and_then(|(_, w)| number(w)))
            .collect::<Option<Vec<_>>>()?;
        let first = value(doc, font, b"FirstChar")
            .and_then(|first| first.as_i64().ok())
            .unwrap_or(0);
        let missing = value(doc, font, b"FontDescriptor")
            .and_then(|descriptor| descriptor.as_dict().ok())
            .and_then(|descriptor| value(doc, descriptor, b"MissingWidth"))
            .and_then(number)
            .unwrap_or(0.0);
        let scale = if subtype == b"Type3" {
            number(value(doc, font, b"FontMatrix")?.as_array().ok()?.first()?)?
        } else {
            0.001
        };
        Some(Self::Simple {
            first,
            widths,
            missing,
            scale,
        })
    }

    /// 字码和它是不是单字节（字距 `Tw` 只加在单字节的 32 上）。
    fn codes(&self, bytes: &[u8]) -> Option<Vec<(u32, bool)>> {
        match self {
            Self::Simple { .. } => Some(bytes.iter().map(|&b| (u32::from(b), true)).collect()),
            Self::Identity { .. } => (bytes.len() % 2 == 0).then(|| {
                bytes
                    .chunks_exact(2)
                    .map(|pair| (u32::from(u16::from_be_bytes([pair[0], pair[1]])), false))
                    .collect()
            }),
        }
    }

    fn width(&self, code: u32) -> f64 {
        match self {
            Self::Simple {
                first,
                widths,
                missing,
                scale,
            } => {
                let index = i64::from(code) - first;
                usize::try_from(index)
                    .ok()
                    .and_then(|index| widths.get(index))
                    .unwrap_or(missing)
                    * scale
            }
            Self::Identity { default, ranges } => ranges
                .iter()
                .find(|(low, high, _)| (*low..=*high).contains(&code))
                .map_or(*default, |(_, _, width)| *width),
        }
    }
}

fn number(object: &Object) -> Option<f64> {
    match object {
        Object::Integer(value) => Some(*value as f64),
        Object::Real(value) => Some(f64::from(*value)),
        _ => None,
    }
}

fn real(value: f64) -> Object {
    Object::Real(value as f32)
}

fn matrix(args: &[Object]) -> Option<Matrix> {
    let values: Vec<f64> = args.iter().map(number).collect::<Option<_>>()?;
    values.try_into().ok()
}

fn multiply(a: Matrix, b: Matrix) -> Matrix {
    [
        a[0] * b[0] + a[1] * b[2],
        a[0] * b[1] + a[1] * b[3],
        a[2] * b[0] + a[3] * b[2],
        a[2] * b[1] + a[3] * b[3],
        a[4] * b[0] + a[5] * b[2] + b[4],
        a[4] * b[1] + a[5] * b[3] + b[5],
    ]
}

/// 页面坐标里的位移 `(dx, dy)`，换成矩阵 `m` 作用之前的坐标里的位移（只看线性部分）。
fn local_delta(m: Matrix, dx: f64, dy: f64) -> Option<(f64, f64)> {
    let det = m[0] * m[3] - m[1] * m[2];
    if det == 0.0 || !det.is_finite() {
        return None;
    }
    Some(((dx * m[3] - dy * m[2]) / det, (dy * m[0] - dx * m[1]) / det))
}

/// 单位正方形经过 `m` 后的外接框（图片、Form 的落点）。
fn unit_bounds(m: Matrix) -> Rect {
    let corners = [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)]
        .map(|(x, y)| (x * m[0] + y * m[2] + m[4], x * m[1] + y * m[3] + m[5]));
    let left = corners.iter().map(|c| c.0).fold(f64::INFINITY, f64::min);
    let right = corners
        .iter()
        .map(|c| c.0)
        .fold(f64::NEG_INFINITY, f64::max);
    let bottom = corners.iter().map(|c| c.1).fold(f64::INFINITY, f64::min);
    let top = corners
        .iter()
        .map(|c| c.1)
        .fold(f64::NEG_INFINITY, f64::max);
    Rect {
        x: left,
        y: bottom,
        width: right - left,
        height: top - bottom,
    }
}

fn close(a: Rect, b: Rect) -> bool {
    let tolerance = 1.0_f64.max(0.01 * a.width.max(a.height));
    [
        (a.x, b.x),
        (a.y, b.y),
        (a.x + a.width, b.x + b.width),
        (a.y + a.height, b.y + b.height),
    ]
    .iter()
    .all(|(p, q)| (p - q).abs() <= tolerance)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::dictionary;

    fn sample(content: &str) -> (Document, ObjectId) {
        let mut doc = Document::with_version("1.7");
        let font = doc.add_object(dictionary! {
            "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
            "FirstChar" => 32, "Widths" => (32..=122).map(|_| Object::Integer(500)).collect::<Vec<_>>(),
        });
        let image = doc.add_object(Stream::new(
            dictionary! { "Type" => "XObject", "Subtype" => "Image", "Width" => 1, "Height" => 1,
            "ColorSpace" => "DeviceGray", "BitsPerComponent" => 8 },
            vec![0],
        ));
        let contents = doc.add_object(Stream::new(Dictionary::new(), content.as_bytes().to_vec()));
        let page = doc.add_object(dictionary! {
            "Type" => "Page", "Contents" => contents,
            "Resources" => dictionary! {
                "Font" => dictionary! { "F1" => font },
                "XObject" => dictionary! { "Im" => image },
            },
        });
        (doc, page)
    }

    fn written(doc: &mut Document, content: &PageContent, page: ObjectId) -> String {
        content.write(doc, page).unwrap();
        let bytes = doc.get_page_content(page).unwrap();
        String::from_utf8(bytes).unwrap()
    }

    #[test]
    fn counts_objects_like_pdfium() {
        let (doc, page) = sample(concat!(
            "BT /F1 10 Tf 0 0 Td (a) Tj () Tj [-200] TJ [(b) 10 (c)] TJ ET ",
            "0 0 m S 0 0 m 1 1 l S 0 0 1 1 re W n q 20 0 0 10 5 6 cm /Im Do Q /Missing Do"
        ));
        let content = PageContent::parse(&doc, page).unwrap();
        let kinds: Vec<_> = content.items.iter().map(|item| item.kind).collect();
        assert_eq!(kinds, [Kind::Text, Kind::Text, Kind::Path, Kind::Image]);
        assert!(content.items[0].continued);
        assert!(!content.items[1].continued);
        let bounds = content.items[3].bounds.unwrap();
        assert_eq!(
            (bounds.x, bounds.y, bounds.width, bounds.height),
            (5.0, 6.0, 20.0, 10.0)
        );
    }

    #[test]
    fn keeps_following_text_in_place() {
        let (mut doc, page) =
            sample("BT /F1 10 Tf 2 0 0 2 0 0 Tm (ab) Tj (cd) Tj 0 -12 Td (ef) Tj ET");
        let mut content = PageContent::parse(&doc, page).unwrap();
        content.delete(&doc, 0).unwrap();
        content.translate(&doc, 2, 4.0, 6.0).unwrap();
        let text = written(&mut doc, &content, page);
        // "ab" 宽 2 × 500 / 1000 × 10 = 10，用 TJ 补回；移动 (4, 6) 在 2 倍的文字矩阵里是 (2, 3)。
        assert!(text.contains("[-1000] TJ\n(cd) Tj"), "{text}");
        assert!(
            text.contains("[-200] TJ\n3 Ts\n(ef) Tj\n0 Ts\n[200] TJ"),
            "{text}"
        );
    }

    #[test]
    fn moves_and_drops_images_and_covers_areas() {
        let (mut doc, page) = sample("q 1 0 0 1 0 0 cm q 2 0 0 2 0 0 cm /Im Do Q");
        let mut content = PageContent::parse(&doc, page).unwrap();
        content.translate(&doc, 0, 4.0, 2.0).unwrap();
        content.cover(&[Rect {
            x: 1.0,
            y: 2.0,
            width: 3.0,
            height: 4.0,
        }]);
        let text = written(&mut doc, &content, page);
        assert!(text.contains("q\n1 0 0 1 2 1 cm\n/Im Do\nQ"), "{text}");
        // 原内容少一个 Q：补齐后再盖黑框。
        assert!(
            text.trim_end()
                .ends_with("Q\nQ\nQ\nq\n0 g\n1 2 3 4 re\nf\nQ"),
            "{text}"
        );

        let mut content = PageContent::parse(&doc, page).unwrap();
        content.delete(&doc, 0).unwrap();
        written(&mut doc, &content, page);
        let resources = doc.get_dictionary(page).unwrap().get(b"Resources").unwrap();
        assert!(!resources.as_dict().unwrap().has(b"XObject"));
    }

    #[test]
    fn refuses_unknown_widths_and_inline_images() {
        let (mut doc, page) = sample("BT /F2 10 Tf (ab) Tj (cd) Tj ET");
        let fonts = dictionary! { "F2" => dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica" } };
        doc.get_dictionary_mut(page)
            .unwrap()
            .set("Resources", dictionary! { "Font" => fonts });
        let mut content = PageContent::parse(&doc, page).unwrap();
        assert!(content.delete(&doc, 0).is_err());
        assert!(content.delete(&doc, 1).is_ok());

        let (doc, page) = sample("BI /W 1 /H 1 /CS /G /BPC 8 ID \x00 EI");
        assert!(PageContent::parse(&doc, page).is_none());
    }
}
