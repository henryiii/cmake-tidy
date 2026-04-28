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
        let parsed = parse_file(
            "cmake_minimum_required(VERSION 3.30)\nproject(example LANGUAGES C CXX)\n",
        );

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
}

