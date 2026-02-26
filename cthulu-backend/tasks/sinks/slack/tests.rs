use super::blocks::*;
use super::markdown::*;

// --- Webhook (mrkdwn) tests ---

#[test]
fn test_headers_to_bold() {
    assert_eq!(markdown_to_slack("# Big Header"), "*Big Header*");
    assert_eq!(markdown_to_slack("## Section"), "*Section*");
    assert_eq!(markdown_to_slack("### Sub"), "*Sub*");
}

#[test]
fn test_bullets() {
    assert_eq!(markdown_to_slack("- item one"), "• item one");
    assert_eq!(markdown_to_slack("* item two"), "• item two");
}

#[test]
fn test_bold() {
    assert_eq!(markdown_to_slack("this is **bold** text"), "this is *bold* text");
}

#[test]
fn test_links() {
    assert_eq!(
        markdown_to_slack("check [this](https://example.com) out"),
        "check <https://example.com|this> out"
    );
}

#[test]
fn test_combined() {
    let md = "# Daily Brief\n\n- **BTC** up 5%\n- Check [CoinDesk](https://coindesk.com)\n\nThis is a paragraph.";
    let expected = "*Daily Brief*\n\n• *BTC* up 5%\n• Check <https://coindesk.com|CoinDesk>\n\nThis is a paragraph.";
    assert_eq!(markdown_to_slack(md), expected);
}

#[test]
fn test_passthrough() {
    assert_eq!(markdown_to_slack("plain text"), "plain text");
}

// --- Block Kit tests ---

#[test]
fn test_markdown_to_blocks_header() {
    let blocks = markdown_to_blocks("# My Header\nSome text");
    assert!(blocks.len() >= 2);
    match &blocks[0] {
        Block::Header { text } => {
            assert_eq!(text.kind, "plain_text");
            // May have emoji prefix
            assert!(text.text.contains("My Header"));
        }
        _ => panic!("expected Header block"),
    }
    match &blocks[1] {
        Block::Section { text } => {
            assert_eq!(text.kind, "mrkdwn");
            assert!(text.text.contains("Some text"));
        }
        _ => panic!("expected Section block"),
    }
}

#[test]
fn test_markdown_to_blocks_divider() {
    let blocks = markdown_to_blocks("Above\n---\nBelow");
    assert!(blocks.len() >= 3);
    assert!(matches!(blocks[1], Block::Divider));
}

#[test]
fn test_markdown_to_blocks_bullets_converted() {
    let blocks = markdown_to_blocks("- item one\n- item two");
    match &blocks[0] {
        Block::RichText { elements } => {
            match &elements[0] {
                RichTextElement::List { style: _, elements: items } => {
                    assert_eq!(items.len(), 2);
                }
                _ => panic!("expected RichTextList"),
            }
        }
        _ => panic!("expected RichText block, got {:?}", blocks[0]),
    }
}

#[test]
fn test_markdown_to_blocks_links_and_bold() {
    let blocks = markdown_to_blocks("Check **this** [link](https://example.com)");
    match &blocks[0] {
        Block::Section { text } => {
            assert!(text.text.contains("*this*"));
            assert!(text.text.contains("<https://example.com|link>"));
        }
        _ => panic!("expected Section"),
    }
}

#[test]
fn test_markdown_to_blocks_long_header_truncated() {
    let long_header = format!("# {}", "A".repeat(200));
    let blocks = markdown_to_blocks(&long_header);
    match &blocks[0] {
        Block::Header { text } => {
            assert!(text.text.len() <= MAX_HEADER_LEN);
        }
        _ => panic!("expected Header"),
    }
}

#[test]
fn test_block_kit_serialization() {
    let block = Block::Header {
        text: TextObject {
            kind: "plain_text",
            text: "Hello".to_string(),
        },
    };
    let json = serde_json::to_value(&block).unwrap();
    assert_eq!(json["type"], "header");
    assert_eq!(json["text"]["type"], "plain_text");
    assert_eq!(json["text"]["text"], "Hello");
}

// --- New tests ---

#[test]
fn test_h2_becomes_bold_section() {
    let blocks = markdown_to_blocks("## My Section");
    match &blocks[0] {
        Block::Section { text } => {
            assert_eq!(text.kind, "mrkdwn");
            assert_eq!(text.text, "*My Section*");
        }
        _ => panic!("expected Section block, got {:?}", blocks[0]),
    }

    let blocks = markdown_to_blocks("### Sub Section");
    match &blocks[0] {
        Block::Section { text } => {
            assert_eq!(text.text, "*Sub Section*");
        }
        _ => panic!("expected Section block"),
    }
}

#[test]
fn test_bullets_become_rich_text_list() {
    let blocks = markdown_to_blocks("- alpha\n- beta\n- gamma");
    match &blocks[0] {
        Block::RichText { elements } => {
            assert_eq!(elements.len(), 1);
            match &elements[0] {
                RichTextElement::List { style: _, elements: items } => {
                    assert_eq!(items.len(), 3);
                    // First item should contain "alpha"
                    let text = super::extract_inline_text(&items[0].elements);
                    assert_eq!(text, "alpha");
                }
                _ => panic!("expected list"),
            }
        }
        _ => panic!("expected RichText block"),
    }
}

#[test]
fn test_context_block_serialization() {
    let block = Block::Context {
        elements: vec![ContextElement::Mrkdwn {
            text: "5 PRs merged".to_string(),
        }],
    };
    let json = serde_json::to_value(&block).unwrap();
    assert_eq!(json["type"], "context");
    assert_eq!(json["elements"][0]["type"], "mrkdwn");
    assert_eq!(json["elements"][0]["text"], "5 PRs merged");
}

#[test]
fn test_rich_text_serialization() {
    let block = Block::RichText {
        elements: vec![RichTextElement::List {
            style: ListStyle::Bullet,
            elements: vec![RichTextListItem {
                elements: vec![RichTextInline::Text {
                    text: "hello".to_string(),
                    style: Some(RichTextStyle { bold: true, ..Default::default() }),
                }],
            }],
        }],
    };
    let json = serde_json::to_value(&block).unwrap();
    assert_eq!(json["type"], "rich_text");
    assert_eq!(json["elements"][0]["type"], "rich_text_list");
    assert_eq!(json["elements"][0]["style"], "bullet");
    assert_eq!(json["elements"][0]["elements"][0]["type"], "rich_text_section");
    assert_eq!(json["elements"][0]["elements"][0]["elements"][0]["type"], "text");
    assert_eq!(json["elements"][0]["elements"][0]["elements"][0]["text"], "hello");
    assert_eq!(json["elements"][0]["elements"][0]["elements"][0]["style"]["bold"], true);
}

#[test]
fn test_emoji_prefix_on_headers() {
    let blocks = markdown_to_blocks("# Dev Changelog, Week of Jan 6");
    match &blocks[0] {
        Block::Header { text } => {
            assert!(text.text.starts_with(":memo:"), "expected emoji prefix, got: {}", text.text);
        }
        _ => panic!("expected Header"),
    }

    // No double-prefix
    let blocks = markdown_to_blocks("# :ship: Already has emoji");
    match &blocks[0] {
        Block::Header { text } => {
            assert!(!text.text.starts_with(":ship: :"), "should not double-prefix: {}", text.text);
        }
        _ => panic!("expected Header"),
    }
}

#[test]
fn test_metadata_becomes_context() {
    let blocks = markdown_to_blocks("5 PRs merged across 3 repos");
    match &blocks[0] {
        Block::Context { elements } => {
            match &elements[0] {
                ContextElement::Mrkdwn { text } => {
                    assert!(text.contains("5 PRs merged"));
                }
            }
        }
        _ => panic!("expected Context block, got {:?}", blocks[0]),
    }
}

#[test]
fn test_parse_inline_bold() {
    let elems = parse_inline_elements("hello **world** end");
    assert_eq!(elems.len(), 3);
    match &elems[0] {
        RichTextInline::Text { text, style } => {
            assert_eq!(text, "hello ");
            assert!(style.is_none());
        }
        _ => panic!("expected Text"),
    }
    match &elems[1] {
        RichTextInline::Text { text, style } => {
            assert_eq!(text, "world");
            assert!(style.as_ref().unwrap().bold);
        }
        _ => panic!("expected bold Text"),
    }
}

#[test]
fn test_parse_inline_link() {
    let elems = parse_inline_elements("see [docs](https://example.com) here");
    assert_eq!(elems.len(), 3);
    match &elems[1] {
        RichTextInline::Link { url, text, .. } => {
            assert_eq!(url, "https://example.com");
            assert_eq!(text.as_deref(), Some("docs"));
        }
        _ => panic!("expected Link"),
    }
}

#[test]
fn test_parse_inline_emoji() {
    let elems = parse_inline_elements("hello :wave: world");
    assert_eq!(elems.len(), 3);
    match &elems[1] {
        RichTextInline::Emoji { name } => {
            assert_eq!(name, "wave");
        }
        _ => panic!("expected Emoji"),
    }
}

#[test]
fn test_parse_inline_code() {
    let elems = parse_inline_elements("run `cargo test` now");
    assert_eq!(elems.len(), 3);
    match &elems[1] {
        RichTextInline::Text { text, style } => {
            assert_eq!(text, "cargo test");
            assert!(style.as_ref().unwrap().code);
        }
        _ => panic!("expected code Text"),
    }
}

#[test]
fn test_mixed_bullets_and_paragraphs() {
    let blocks = markdown_to_blocks("Some intro\n\n- bullet one\n- bullet two\n\nAnother paragraph");
    // Should be: Section, RichText, Section
    assert!(blocks.len() >= 3, "got {} blocks: {:?}", blocks.len(), blocks);
    assert!(matches!(&blocks[0], Block::Section { .. }));
    assert!(matches!(&blocks[1], Block::RichText { .. }));
    assert!(matches!(&blocks[2], Block::Section { .. }));
}

#[test]
fn test_divider_serialization() {
    let block = Block::Divider;
    let json = serde_json::to_value(&block).unwrap();
    assert_eq!(json["type"], "divider");
    // Should only have "type" key
    assert_eq!(json.as_object().unwrap().len(), 1);
}

#[test]
fn test_section_serialization() {
    let block = Block::Section {
        text: TextObject {
            kind: "mrkdwn",
            text: "*bold section*".to_string(),
        },
    };
    let json = serde_json::to_value(&block).unwrap();
    assert_eq!(json["type"], "section");
    assert_eq!(json["text"]["type"], "mrkdwn");
    assert_eq!(json["text"]["text"], "*bold section*");
}

#[test]
fn test_stats_block_produces_section_fields() {
    let md = "Some intro\n\n[stats]\n:rocket: *12* PRs merged | :file_folder: *45* files\n:heavy_plus_sign: *3,450* added | :heavy_minus_sign: *1,200* removed\n[/stats]\n\nAfter stats";
    let blocks = markdown_to_blocks(md);

    // Find the SectionFields block
    let sf = blocks.iter().find(|b| matches!(b, Block::SectionFields { .. }));
    assert!(sf.is_some(), "expected SectionFields block in {:?}", blocks);

    match sf.unwrap() {
        Block::SectionFields { fields } => {
            assert_eq!(fields.len(), 4);
            assert!(fields[0].text.contains("12"));
            assert!(fields[1].text.contains("45"));
            assert!(fields[2].text.contains("3,450"));
            assert!(fields[3].text.contains("1,200"));
            assert_eq!(fields[0].kind, "mrkdwn");
        }
        _ => unreachable!(),
    }
}

#[test]
fn test_stats_block_serialization() {
    let block = Block::SectionFields {
        fields: vec![
            TextObject { kind: "mrkdwn", text: ":rocket: *12* PRs".to_string() },
            TextObject { kind: "mrkdwn", text: ":file_folder: *45* files".to_string() },
        ],
    };
    let json = serde_json::to_value(&block).unwrap();
    assert_eq!(json["type"], "section");
    assert!(json.get("text").is_none(), "SectionFields should not have text key");
    assert_eq!(json["fields"].as_array().unwrap().len(), 2);
    assert_eq!(json["fields"][0]["type"], "mrkdwn");
    assert_eq!(json["fields"][0]["text"], ":rocket: *12* PRs");
}

#[test]
fn test_stats_block_single_column_lines() {
    let md = "[stats]\n:rocket: *12* PRs merged\n:file_folder: *45* files changed\n[/stats]";
    let blocks = markdown_to_blocks(md);
    match &blocks[0] {
        Block::SectionFields { fields } => {
            assert_eq!(fields.len(), 2);
        }
        _ => panic!("expected SectionFields, got {:?}", blocks[0]),
    }
}

#[test]
fn test_stats_block_surrounded_by_content() {
    let md = "# Changelog\n\n- highlight one\n- highlight two\n\n[stats]\n:rocket: *5* merged | :package: *2* repos\n[/stats]";
    let blocks = markdown_to_blocks(md);

    assert!(matches!(&blocks[0], Block::Header { .. }));
    assert!(matches!(&blocks[1], Block::RichText { .. }));
    assert!(matches!(&blocks[2], Block::SectionFields { .. }));
}
