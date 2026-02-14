use std::collections::HashMap;

pub fn render_prompt(template: &str, vars: &HashMap<String, String>) -> String {
    let mut result = template.to_string();
    for (key, value) in vars {
        let placeholder = format!("{{{{{}}}}}", key);
        result = result.replace(&placeholder, value);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_substitution() {
        let template = "Hello {{name}}!";
        let mut vars = HashMap::new();
        vars.insert("name".to_string(), "world".to_string());
        assert_eq!(render_prompt(template, &vars), "Hello world!");
    }

    #[test]
    fn test_multiple_variables() {
        let template = "PR #{{pr_number}} in {{repo}} by {{author}}";
        let mut vars = HashMap::new();
        vars.insert("pr_number".to_string(), "42".to_string());
        vars.insert("repo".to_string(), "owner/repo".to_string());
        vars.insert("author".to_string(), "alice".to_string());
        let result = render_prompt(template, &vars);
        assert_eq!(result, "PR #42 in owner/repo by alice");
    }

    #[test]
    fn test_repeated_variable() {
        let template = "{{repo}} - reviewing {{repo}}";
        let mut vars = HashMap::new();
        vars.insert("repo".to_string(), "my/repo".to_string());
        assert_eq!(render_prompt(template, &vars), "my/repo - reviewing my/repo");
    }

    #[test]
    fn test_no_variables_passthrough() {
        let template = "No variables here.";
        let vars = HashMap::new();
        assert_eq!(render_prompt(template, &vars), "No variables here.");
    }

    #[test]
    fn test_unmatched_placeholder_left_intact() {
        let template = "Known: {{known}}, Unknown: {{unknown}}";
        let mut vars = HashMap::new();
        vars.insert("known".to_string(), "yes".to_string());
        let result = render_prompt(template, &vars);
        assert_eq!(result, "Known: yes, Unknown: {{unknown}}");
    }

    #[test]
    fn test_empty_value() {
        let template = "Before{{var}}After";
        let mut vars = HashMap::new();
        vars.insert("var".to_string(), "".to_string());
        assert_eq!(render_prompt(template, &vars), "BeforeAfter");
    }

    #[test]
    fn test_multiline_diff() {
        let template = "```diff\n{{diff}}\n```";
        let mut vars = HashMap::new();
        vars.insert("diff".to_string(), "+added\n-removed\n context".to_string());
        let result = render_prompt(template, &vars);
        assert!(result.contains("+added\n-removed\n context"));
    }

    #[test]
    fn test_value_containing_braces() {
        let template = "Output: {{output}}";
        let mut vars = HashMap::new();
        vars.insert("output".to_string(), "fn main() { println!(\"hello\"); }".to_string());
        let result = render_prompt(template, &vars);
        assert!(result.contains("fn main()"));
    }

    #[test]
    fn test_extra_vars_ignored() {
        let template = "Hello {{name}}";
        let mut vars = HashMap::new();
        vars.insert("name".to_string(), "world".to_string());
        vars.insert("unused".to_string(), "ignored".to_string());
        assert_eq!(render_prompt(template, &vars), "Hello world");
    }
}
