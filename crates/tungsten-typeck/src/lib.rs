pub mod checker;
pub mod effects;
pub mod interval;
pub mod types;

use checker::{TypeChecker, TypeError};
use tungsten_syntax::ast::Program;

pub fn check(program: &Program) -> Result<(), Vec<TypeError>> {
    let mut checker = TypeChecker::new();
    checker.check_program(program)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tungsten_syntax::parse;

    #[test]
    fn test_valid_refinement_and_effects() {
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
            } with Db {
                query(sql, args) => PostgresPool::execute(sql, args).await
            } with IOError {
                err => println!("Failed: {}", err),
            }
        }
        "#;

        let ast = parse(code).expect("syntax parse ok");
        let res = check(&ast);
        assert!(res.is_ok(), "Type checking should succeed: {:?}", res.err());
    }

    #[test]
    fn test_unhandled_effect_rejection() {
        let code = r#"
        fn effectful() -> () yields [Db] {
            Db::query("SELECT 1", 0)?;
        }

        fn unhandled_caller() {
            effectful(); // Should be rejected because caller does not declare yields [Db] and does not handle it
        }
        "#;

        let ast = parse(code).expect("syntax parse ok");
        let res = check(&ast);
        assert!(res.is_err(), "Should reject unhandled effect");
        let errs = res.unwrap_err();
        assert!(errs.iter().any(|e| e.message.contains("Unhandled algebraic effect 'Db'")));
    }
}
