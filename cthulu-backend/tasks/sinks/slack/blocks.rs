use serde::ser::{SerializeMap, Serializer};
use serde::Serialize;

pub const MAX_HEADER_LEN: usize = 150;
pub const MAX_SECTION_LEN: usize = 3000;
pub const MAX_BLOCKS_PER_MESSAGE: usize = 50;

// ---------------------------------------------------------------------------
// Block Kit types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum Block {
    Header { text: TextObject },
    Section { text: TextObject },
    SectionFields { fields: Vec<TextObject> },
    Context { elements: Vec<ContextElement> },
    RichText { elements: Vec<RichTextElement> },
    Divider,
}

impl Serialize for Block {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        match self {
            Block::Header { text } => {
                let mut map = serializer.serialize_map(Some(2))?;
                map.serialize_entry("type", "header")?;
                map.serialize_entry("text", text)?;
                map.end()
            }
            Block::Section { text } => {
                let mut map = serializer.serialize_map(Some(2))?;
                map.serialize_entry("type", "section")?;
                map.serialize_entry("text", text)?;
                map.end()
            }
            Block::SectionFields { fields } => {
                let mut map = serializer.serialize_map(Some(2))?;
                map.serialize_entry("type", "section")?;
                map.serialize_entry("fields", fields)?;
                map.end()
            }
            Block::Context { elements } => {
                let mut map = serializer.serialize_map(Some(2))?;
                map.serialize_entry("type", "context")?;
                map.serialize_entry("elements", elements)?;
                map.end()
            }
            Block::RichText { elements } => {
                let mut map = serializer.serialize_map(Some(2))?;
                map.serialize_entry("type", "rich_text")?;
                map.serialize_entry("elements", elements)?;
                map.end()
            }
            Block::Divider => {
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry("type", "divider")?;
                map.end()
            }
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TextObject {
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub text: String,
}

// -- Context block elements --

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContextElement {
    #[serde(rename = "mrkdwn")]
    Mrkdwn { text: String },
}

// -- Rich text block elements --

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum RichTextElement {
    Section {
        elements: Vec<RichTextInline>,
    },
    List {
        style: ListStyle,
        elements: Vec<RichTextListItem>,
    },
}

impl Serialize for RichTextElement {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        match self {
            RichTextElement::Section { elements } => {
                let mut map = serializer.serialize_map(Some(2))?;
                map.serialize_entry("type", "rich_text_section")?;
                map.serialize_entry("elements", elements)?;
                map.end()
            }
            RichTextElement::List { style, elements } => {
                let mut map = serializer.serialize_map(Some(3))?;
                map.serialize_entry("type", "rich_text_list")?;
                map.serialize_entry(
                    "style",
                    match style {
                        ListStyle::Bullet => "bullet",
                        ListStyle::Ordered => "ordered",
                    },
                )?;
                map.serialize_entry("elements", elements)?;
                map.end()
            }
        }
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum ListStyle {
    Bullet,
    Ordered,
}

#[derive(Debug, Clone)]
pub struct RichTextListItem {
    pub elements: Vec<RichTextInline>,
}

impl Serialize for RichTextListItem {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(2))?;
        map.serialize_entry("type", "rich_text_section")?;
        map.serialize_entry("elements", &self.elements)?;
        map.end()
    }
}

#[derive(Debug, Clone)]
pub enum RichTextInline {
    Text {
        text: String,
        style: Option<RichTextStyle>,
    },
    Link {
        url: String,
        text: Option<String>,
        style: Option<RichTextStyle>,
    },
    Emoji {
        name: String,
    },
}

impl Serialize for RichTextInline {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        match self {
            RichTextInline::Text { text, style } => {
                let has_style = style.as_ref().is_some_and(|s| s.has_any());
                let count = 2 + usize::from(has_style);
                let mut map = serializer.serialize_map(Some(count))?;
                map.serialize_entry("type", "text")?;
                map.serialize_entry("text", text)?;
                if let Some(s) = style {
                    if s.has_any() {
                        map.serialize_entry("style", s)?;
                    }
                }
                map.end()
            }
            RichTextInline::Link { url, text, style } => {
                let has_style = style.as_ref().is_some_and(|s| s.has_any());
                let mut count = 2;
                if text.is_some() {
                    count += 1;
                }
                if has_style {
                    count += 1;
                }
                let mut map = serializer.serialize_map(Some(count))?;
                map.serialize_entry("type", "link")?;
                map.serialize_entry("url", url)?;
                if let Some(t) = text {
                    map.serialize_entry("text", t)?;
                }
                if let Some(s) = style {
                    if s.has_any() {
                        map.serialize_entry("style", s)?;
                    }
                }
                map.end()
            }
            RichTextInline::Emoji { name } => {
                let mut map = serializer.serialize_map(Some(2))?;
                map.serialize_entry("type", "emoji")?;
                map.serialize_entry("name", name)?;
                map.end()
            }
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct RichTextStyle {
    pub bold: bool,
    pub italic: bool,
    pub code: bool,
}

impl RichTextStyle {
    pub(crate) fn has_any(&self) -> bool {
        self.bold || self.italic || self.code
    }
}

impl Serialize for RichTextStyle {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        let count = usize::from(self.bold) + usize::from(self.italic) + usize::from(self.code);
        let mut map = serializer.serialize_map(Some(count))?;
        if self.bold {
            map.serialize_entry("bold", &true)?;
        }
        if self.italic {
            map.serialize_entry("italic", &true)?;
        }
        if self.code {
            map.serialize_entry("code", &true)?;
        }
        map.end()
    }
}
