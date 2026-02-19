pub mod keyword;

use crate::tasks::sources::ContentItem;

pub trait Filter: Send + Sync {
    fn apply(&self, items: Vec<ContentItem>) -> Vec<ContentItem>;
}
