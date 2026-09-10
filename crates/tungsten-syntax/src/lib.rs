pub mod ast;
pub mod fmt;
pub mod lexer;
pub mod parser;
pub mod token;

use ast::Program;
use lexer::Lexer;
use parser::Parser;

pub use fmt::{format, format_source};

pub fn parse(source: &str) -> Result<Program, String> {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize()?;
    let mut parser = Parser::new(tokens);
    parser.parse_program()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_player_example() {
        let code = r#"
        type Health = u8(0..=100);

        struct Player {
            name: String,
            hp: Health,
        }

        fn fetch_player(id: u64) -> Player yields [Db, IOError] {
            let record = Db::query("SELECT * FROM players WHERE id = ?", id)?;
            
            Player {
                name: record.name,
                hp: record.hp as Health,
            }
        }

        fn heal_player(player: &mut Player, amount: u8) {
            player.hp = player.hp.saturating_add(amount);
        }

        fn main() {
            handle {
                let mut player = fetch_player(42)!;
                heal_player(&mut player, 20);
                println!("Player {} has {} HP", player.name, player.hp);
            } with Db {
                query(sql, args) => PostgresPool::execute(sql, args).await
            } with IOError {
                err => println!("Failed to fetch player: {}", err),
            }
        }
        "#;

        let program = parse(code).expect("Should parse player example successfully");
        assert_eq!(program.items.len(), 5);
    }

    #[test]
    fn test_formatter_idempotency() {
        let code = r#"
        type Health = u8(0..=100);

        struct Player {
            name: String,
            hp: Health,
        }

        fn fetch_player(id: u64) -> Player yields [Db, IOError] {
            let record = Db::query("SELECT * FROM players WHERE id = ?", id)?;
            Player {
                name: record.name,
                hp: record.hp as Health,
            }
        }
        "#;

        let formatted1 = format_source(code).expect("first format ok");
        let formatted2 = format_source(&formatted1).expect("second format ok");
        assert_eq!(formatted1, formatted2, "Formatter must be idempotent");
    }
}
