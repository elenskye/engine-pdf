//! PDF 改动单。所有坐标都使用原 PDF 的左下原点和点单位。

use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::{Cancel, Document, Error};

/// App 和编辑套件共用的改动单。
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct Plan {
    /// 输出页序。省略时保留主文档的全部页面及顺序。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pages: Option<Vec<Pick>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub add: Vec<Mark>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub remove: Vec<MarkId>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub fill: Vec<Fill>,
    /// 替换整棵目录。目标页码引用输出；空数组删除目录。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outline: Option<Vec<OutlineItem>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub info: Option<InfoPatch>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pick {
    /// 主文档是 0，附加来源从 1 起。
    pub source: u32,
    /// 来源文件内的物理页码，从 1 起。
    pub page: u32,
    /// 在原页面旋转上追加的顺时针角度，必须是 90 的倍数。
    #[serde(default)]
    pub rotate: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Rect {
    /// 将原点在左上的点坐标换成左下原点的 PDF 坐标。
    pub fn from_top(page_height: f64, x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            x,
            y: page_height - y - height,
            width,
            height,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Quad {
    pub top_left: Point,
    pub top_right: Point,
    pub bottom_left: Point,
    pub bottom_right: Point,
}

/// 四个分量都在 0 到 1 之间。
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Color {
    pub red: f64,
    pub green: f64,
    pub blue: f64,
    pub alpha: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Stroke {
    pub width: f64,
    pub color: Color,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Mark {
    /// 来源编号与 `Pick::source` 一致。
    pub source: u32,
    /// 来源文件内的物理页码。复制该页时，每份副本都会带上这项标注。
    pub page: u32,
    pub rect: Rect,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contents: Option<String>,
    #[serde(flatten)]
    pub kind: MarkKind,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum MarkKind {
    Highlight {
        quads: Vec<Quad>,
        color: Color,
    },
    Underline {
        quads: Vec<Quad>,
        color: Color,
    },
    Strikeout {
        quads: Vec<Quad>,
        color: Color,
    },
    Note {
        color: Color,
    },
    TextBox {
        text: String,
        font_size: f64,
        color: Color,
        border: Option<Stroke>,
    },
    Ink {
        strokes: Vec<Vec<Point>>,
        stroke: Stroke,
    },
    Shape {
        shape: Shape,
        stroke: Stroke,
        fill: Option<Color>,
    },
    Image {
        format: StampFormat,
        bytes: Vec<u8>,
    },
    /// 内部目标使用输出物理页码。
    Link {
        target: Target,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Shape {
    Rectangle,
    Ellipse,
    Line { start: Point, end: Point },
    Arrow { start: Point, end: Point },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StampFormat {
    Png,
    Jpeg,
}

/// 现有标注在原始文件中的位置。在同一份原始字节上使用。
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MarkId {
    pub source: u32,
    pub page: u32,
    /// 可读取的页内标注顺序，从 0 起。跳过弹窗和表单控件。
    pub index: u32,
}

/// 已有标注在原始页面中的位置、种类及批注信息。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Annotation {
    pub id: MarkId,
    pub rect: Rect,
    pub author: Option<String>,
    pub contents: Option<String>,
    pub color: Option<Color>,
    #[serde(flatten)]
    pub kind: AnnotationKind,
    /// 可用于重建该标注的数据。图章按实际外观返回 PNG。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mark: Option<MarkKind>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum AnnotationKind {
    Highlight,
    Underline,
    Strikeout,
    Note,
    TextBox,
    Ink,
    Rectangle,
    Ellipse,
    Line,
    Arrow,
    Image,
    Link { target: Target },
    Other { subtype: String },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Fill {
    pub name: String,
    pub value: FieldValue,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FieldValue {
    /// 文本、单选值或下拉选项的导出值。
    Text(String),
    Checked(bool),
}

/// 一个有全限定名称的表单字段，以及属于该字段的各个控件。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Field {
    pub name: String,
    pub kind: FieldKind,
    pub value: Option<FieldValue>,
    pub options: Vec<FieldOption>,
    pub widgets: Vec<FieldWidget>,
    pub read_only: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FieldKind {
    Text,
    Checkbox,
    Radio,
    Dropdown,
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldOption {
    pub value: String,
    pub label: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct FieldWidget {
    pub page: u32,
    pub rect: Rect,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OutlineItem {
    pub title: String,
    pub target: Target,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<OutlineItem>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Target {
    /// 物理页码从 1 起，`y` 为 PDF 坐标。
    Page {
        page: u32,
        y: f64,
    },
    Url {
        url: String,
    },
    None,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Info {
    pub title: Option<String>,
    pub author: Option<String>,
    pub subject: Option<String>,
    pub keywords: Vec<String>,
    pub creator: Option<String>,
    pub producer: Option<String>,
    /// 自 Unix 纪元起的秒数。
    pub created: Option<f64>,
    pub modified: Option<f64>,
    pub encrypted: bool,
    pub permissions: Permissions,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct InfoPatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keywords: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub creator: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Permissions {
    pub print: bool,
    pub copy: bool,
    pub modify: bool,
    pub annotate: bool,
}

impl Default for Permissions {
    fn default() -> Self {
        Self {
            print: true,
            copy: true,
            modify: true,
            annotate: true,
        }
    }
}

/// 保存选项由 Rust 调用方提供。
#[derive(Default)]
pub struct Save {
    /// 输出加打开密码、权限密码和权限。
    pub lock: Option<Lock>,
    /// 把表单和除链接外的标注烧进页面内容；调整页序的文件有表单时必须开。
    pub flatten: bool,
    /// 用权限密码打开后，显式输出无密码的 PDF。
    pub remove_password: bool,
}

pub struct Lock {
    pub owner: Zeroizing<String>,
    pub user: Option<Zeroizing<String>>,
    pub permissions: Permissions,
}

/// 将改动应用到新 PDF 字节，原文件不动。写法见 `decisions/0017`：页序不变时在原文件的对象上改，
/// 其余页内容、页标签、结构树、表单都原样保留；调整页序时把所选页搬进新文件。
pub fn apply(
    doc: &Document,
    others: &[&Document],
    plan: &Plan,
    save: &Save,
    cancel: &Cancel,
) -> Result<Vec<u8>, Error> {
    cancel.check()?;
    let others: Vec<_> = others.iter().map(|doc| doc.native()).collect();
    crate::sys::apply(doc.native(), &others, plan, save, cancel)
}
