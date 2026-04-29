mod parsed;
mod parser;
mod token_source;

pub use parsed::{ParseError, Parsed};
pub use parser::parse_file;

#[cfg(test)]
mod tests {
    use cmake_tidy_ast::{Argument, Statement};

    use crate::parse_file;

    #[test]
    fn parses_simple_command_invocations() {
        let parsed =
            parse_file("cmake_minimum_required(VERSION 3.30)\nproject(example LANGUAGES C CXX)\n");

        assert!(parsed.errors.is_empty());
        assert_eq!(parsed.syntax.items.len(), 2);

        let Statement::Command(command) = &parsed.syntax.items[0];
        assert_eq!(command.name.text, "cmake_minimum_required");
        assert_eq!(command.arguments.len(), 2);

        let Argument::Unquoted(version) = &command.arguments[0] else {
            panic!("expected an unquoted argument");
        };
        assert_eq!(version.text, "VERSION");
    }

    #[test]
    fn parses_nested_parenthesized_arguments() {
        let parsed = parse_file("if((A AND B) OR C)\nendif()");
        assert!(parsed.errors.is_empty());

        let Statement::Command(command) = &parsed.syntax.items[0];
        assert!(matches!(command.arguments[0], Argument::ParenGroup(_)));
    }

    #[test]
    fn reports_missing_left_paren_after_command_name() {
        let parsed = parse_file("project example");

        assert_eq!(parsed.syntax.items.len(), 2);
        assert_eq!(parsed.errors.len(), 2);
        assert_eq!(parsed.errors[0].message, "expected `(` after command name");
        assert_eq!(parsed.errors[1].message, "expected `(` after command name");
    }

    #[test]
    fn reports_missing_closing_paren() {
        let parsed = parse_file("project(example");

        assert_eq!(parsed.syntax.items.len(), 1);
        assert_eq!(parsed.errors.len(), 1);
        assert_eq!(
            parsed.errors[0].message,
            "expected `)` to close command invocation"
        );
    }

    #[test]
    fn reports_non_identifier_at_statement_start() {
        let parsed = parse_file("\"oops\"\nproject(example)\n");

        assert_eq!(parsed.syntax.items.len(), 1);
        assert_eq!(parsed.errors.len(), 1);
        assert_eq!(parsed.errors[0].message, "expected a command name");

        let Statement::Command(command) = &parsed.syntax.items[0];
        assert_eq!(command.name.text, "project");
    }

    #[test]
    fn reports_nested_group_missing_outer_closing_paren() {
        let parsed = parse_file("project(()\n");

        assert_eq!(parsed.syntax.items.len(), 1);
        assert_eq!(parsed.errors.len(), 1);
        assert_eq!(
            parsed.errors[0].message,
            "expected `)` to close command invocation"
        );
    }
}
