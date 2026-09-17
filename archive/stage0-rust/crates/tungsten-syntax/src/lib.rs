pub mod ast;
pub mod fmt;
pub mod lexer;
pub mod parser;
pub mod token;

use ast::Program;
use lexer::Lexer;
use parser::Parser;

pub use fmt::{format, format_expr, format_source};

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

    #[test]
    fn test_parse_and_format_region() {
        let code = r#"
        struct Point {
            x: i64,
            y: i64,
        }

        fn main() {
            region r {
                let p = Point { x: 10, y: 20 };
                println!("Point inside region: {}, {}", p.x, p.y);
            }
        }
        "#;
        let program = parse(code).expect("parse region code ok");
        assert_eq!(program.items.len(), 2);
        let formatted1 = format_source(code).expect("format region ok");
        let formatted2 = format_source(&formatted1).expect("format region idempotent ok");
        assert_eq!(formatted1, formatted2);
    }

    #[test]
    fn test_parse_and_format_nursery() {
        let code = r#"
        fn worker(ch: i64, val: i64) yields [Channel] {
            Channel::send(ch, val);
        }

        fn main() yields [Async, Channel] {
            let ch = Channel::new();
            nursery n {
                n.spawn(worker, ch, 10);
                n.spawn(worker, ch, 20);
            }
        }
        "#;
        let program = parse(code).expect("parse nursery code ok");
        assert_eq!(program.items.len(), 2);
        let formatted1 = format_source(code).expect("format nursery ok");
        let formatted2 = format_source(&formatted1).expect("format nursery idempotent ok");
        assert_eq!(formatted1, formatted2);
    }

    #[test]
    fn test_parse_and_format_imports() {
        let code = r#"
        import math;
        import std::collections;
        import utils::logger as log;

        fn main() {
            println!("Imports parsed successfully");
        }
        "#;
        let program = parse(code).expect("parse imports ok");
        assert_eq!(program.items.len(), 4);
        if let ast::Item::Import(ref imp) = program.items[0] {
            assert_eq!(imp.path, vec!["math".to_string()]);
            assert_eq!(imp.alias, None);
        } else {
            panic!("Expected Import item");
        }
        if let ast::Item::Import(ref imp) = program.items[2] {
            assert_eq!(imp.path, vec!["utils".to_string(), "logger".to_string()]);
            assert_eq!(imp.alias, Some("log".to_string()));
        } else {
            panic!("Expected Import item with alias");
        }
        let formatted1 = format_source(code).expect("format imports ok");
        let formatted2 = format_source(&formatted1).expect("format imports idempotent ok");
        assert_eq!(formatted1, formatted2);
    }

    #[test]
    fn test_parse_and_format_enums_and_arrays() {
        let code = r#"
        pub enum Option<T> {
            Some(T),
            None,
        }

        pub enum Command {
            Quit,
            Move(i64, i64),
        }

        fn process(cmd: Command) -> i64 {
            let res = match cmd {
                Command::Quit => 0,
                Command::Move(x, y) => x + y,
                _ => 99,
            };
            res
        }

        fn main() {
            let arr: [i64; 3] = [10, 20, 30];
            let first = arr[0];
            let opt = Option::Some(first);
        }
        "#;
        let program = parse(code).expect("parse enums and arrays ok");
        assert_eq!(program.items.len(), 4);
        let formatted1 = format_source(code).expect("format enums and arrays ok");
        let formatted2 = format_source(&formatted1).expect("format idempotent ok");
        assert_eq!(formatted1, formatted2);
    }

    #[test]
    fn test_parse_and_format_ffi_and_unsafe() {
        let code = r#"
        extern "C" {
            fn puts(str: *u8) -> i32;
            fn strlen(str: *const u8) -> i64;
        }

        #[repr(C)]
        struct Point {
            x: i64,
            y: i64,
        }

        fn main() {
            let p = Point { x: 10, y: 20 };
            unsafe {
                let ptr: *u8 = &p as *u8;
                let val: u8 = *ptr;
                puts(ptr);
            }
        }
        "#;
        let program = parse(code).expect("parse ffi code ok");
        assert_eq!(program.items.len(), 3);
        if let ast::Item::ExternBlock(ref eb) = program.items[0] {
            assert_eq!(eb.abi, "C");
            assert_eq!(eb.fns.len(), 2);
            assert_eq!(eb.fns[0].name, "puts");
            assert_eq!(eb.fns[1].name, "strlen");
        } else {
            panic!("Expected ExternBlock item");
        }
        if let ast::Item::Struct(ref st) = program.items[1] {
            assert!(st.repr_c);
            assert_eq!(st.name, "Point");
        } else {
            panic!("Expected Struct with repr_c");
        }
        let formatted1 = format_source(code).expect("format ffi ok");
        let formatted2 = format_source(&formatted1).expect("format ffi idempotent ok");
        assert_eq!(formatted1, formatted2);
    }
}


