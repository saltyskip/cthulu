use super::Filter;
use crate::tasks::sources::ContentItem;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchField {
    Title,
    Summary,
    TitleOrSummary,
}

pub struct KeywordFilter {
    keywords: Vec<String>,
    require_all: bool,
    field: MatchField,
}

impl KeywordFilter {
    pub fn new(keywords: Vec<String>, require_all: bool, field: MatchField) -> Self {
        Self {
            keywords: keywords.into_iter().map(|k| k.to_lowercase()).collect(),
            require_all,
            field,
        }
    }

    fn matches_text(&self, text: &str) -> bool {
        let lower = text.to_lowercase();
        if self.require_all {
            self.keywords.iter().all(|kw| lower.contains(kw.as_str()))
        } else {
            self.keywords.iter().any(|kw| lower.contains(kw.as_str()))
        }
    }
}

impl Filter for KeywordFilter {
    fn apply(&self, items: Vec<ContentItem>) -> Vec<ContentItem> {
        items
            .into_iter()
            .filter(|item| match self.field {
                MatchField::Title => self.matches_text(&item.title),
                MatchField::Summary => self.matches_text(&item.summary),
                MatchField::TitleOrSummary => {
                    self.matches_text(&item.title) || self.matches_text(&item.summary)
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_item(title: &str, summary: &str) -> ContentItem {
        ContentItem {
            title: title.to_string(),
            url: String::new(),
            summary: summary.to_string(),
            published: None,
            image_url: None,
        }
    }

    #[test]
    fn test_keyword_filter_any_title() {
        let filter = KeywordFilter::new(
            vec!["bitcoin".into(), "ethereum".into()],
            false,
            MatchField::Title,
        );
        let items = vec![
            make_item("Bitcoin hits new high", "Price surges"),
            make_item("Apple releases new phone", "Tech news"),
            make_item("Ethereum upgrade live", "Network update"),
        ];
        let result = filter.apply(items);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].title, "Bitcoin hits new high");
        assert_eq!(result[1].title, "Ethereum upgrade live");
    }

    #[test]
    fn test_keyword_filter_require_all() {
        let filter = KeywordFilter::new(
            vec!["bitcoin".into(), "etf".into()],
            true,
            MatchField::Title,
        );
        let items = vec![
            make_item("Bitcoin ETF approved", ""),
            make_item("Bitcoin hits high", ""),
            make_item("New ETF launched", ""),
        ];
        let result = filter.apply(items);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].title, "Bitcoin ETF approved");
    }

    #[test]
    fn test_keyword_filter_case_insensitive() {
        let filter = KeywordFilter::new(
            vec!["BTC".into()],
            false,
            MatchField::Title,
        );
        let items = vec![
            make_item("btc price update", ""),
            make_item("BTC Soars", ""),
        ];
        let result = filter.apply(items);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_keyword_filter_summary_field() {
        let filter = KeywordFilter::new(
            vec!["crypto".into()],
            false,
            MatchField::Summary,
        );
        let items = vec![
            make_item("Market update", "Crypto markets rally"),
            make_item("Market update", "Stock markets rally"),
        ];
        let result = filter.apply(items);
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_keyword_filter_title_or_summary() {
        let filter = KeywordFilter::new(
            vec!["sec".into()],
            false,
            MatchField::TitleOrSummary,
        );
        let items = vec![
            make_item("SEC ruling", "Details here"),
            make_item("Market news", "The SEC issued guidance"),
            make_item("Weather forecast", "Sunny skies"),
        ];
        let result = filter.apply(items);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_keyword_filter_empty_keywords() {
        let filter = KeywordFilter::new(
            vec![],
            false,
            MatchField::Title,
        );
        let items = vec![make_item("Anything", "")];
        // With empty keywords and require_all=false, any() returns false
        let result = filter.apply(items);
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_keyword_filter_empty_keywords_require_all() {
        let filter = KeywordFilter::new(
            vec![],
            true,
            MatchField::Title,
        );
        let items = vec![make_item("Anything", "")];
        // With empty keywords and require_all=true, all() returns true
        let result = filter.apply(items);
        assert_eq!(result.len(), 1);
    }
}
