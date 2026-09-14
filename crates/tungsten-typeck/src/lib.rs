pub mod checker;
pub mod effects;
pub mod interval;
pub mod relational;
pub mod types;
pub mod unify;

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

    #[test]
    fn test_generics_and_relational_typechecking() {
        let code = r#"
        struct Box<T> {
            val: T,
        }

        fn wrap<T>(item: T) -> Box<T> {
            Box { val: item }
        }

        fn slice_bounds(start: usize, end: usize(>= start)) -> usize {
            end - start
        }

        fn apply<T, U>(val: T, f: fn(T) -> U) -> U {
            f(val)
        }

        fn double(n: i64) -> i64 {
            n * 2
        }

        fn main() {
            let b = wrap(42);
            let diff = slice_bounds(10, 20);
            let res = apply(21, double);
        }
        "#;

        let ast = parse(code).expect("syntax parse ok");
        let res = check(&ast);
        assert!(res.is_ok(), "Generics and relational type checking should succeed: {:?}", res.err());
    }

    #[test]
    fn test_relational_violation_rejection() {
        let code = r#"
        fn slice_bounds(start: usize, end: usize(>= start)) -> usize {
            end - start
        }

        fn main() {
            // Constant bounds violation: 5 is not >= 10
            let diff = slice_bounds(10, 5);
        }
        "#;

        let ast = parse(code).expect("syntax parse ok");
        let res = check(&ast);
        assert!(res.is_err(), "Should reject invalid relational argument");
        let errs = res.unwrap_err();
        assert!(errs.iter().any(|e| e.message.contains("Relational refinement check failed")));
    }

    #[test]
    fn test_valid_region_allocation() {
        let code = r#"
        struct Point {
            x: i64,
            y: i64,
        }

        fn compute() -> i64 {
            let res = region r {
                let p = Point { x: 10, y: 20 };
                let ref_p = &p;
                ref_p.x + ref_p.y
            };
            res
        }
        "#;

        let ast = parse(code).expect("syntax parse ok");
        let res = check(&ast);
        assert!(res.is_ok(), "Valid region allocation should succeed: {:?}", res.err());
    }

    #[test]
    fn test_region_escape_return_rejection() {
        let code = r#"
        struct Point {
            x: i64,
            y: i64,
        }

        fn escape_attempt() -> &Point {
            let p = Point { x: 10, y: 20 };
            &p
        }
        "#;

        let ast = parse(code).expect("syntax parse ok");
        let res = check(&ast);
        assert!(res.is_err(), "Escaping reference from function should be rejected");
        let errs = res.unwrap_err();
        assert!(errs.iter().any(|e| e.message.contains("Region escape violation")));
    }

    #[test]
    fn test_region_escape_block_rejection() {
        let code = r#"
        struct Point {
            x: i64,
            y: i64,
        }

        fn escape_from_region() {
            let global_p = Point { x: 0, y: 0 };
            let mut outer_ref = &global_p;
            region r {
                let p = Point { x: 10, y: 20 };
                // Assigning inner reference to outer variable violates region lifetime (r_src > r_dst)
                outer_ref = &p;
            };
        }
        "#;

        let ast = parse(code).expect("syntax parse ok");
        let res = check(&ast);
        assert!(res.is_err(), "Escaping reference from region block should be rejected");
        let errs = res.unwrap_err();
        assert!(errs.iter().any(|e| e.message.contains("Region escape violation")));
    }

    #[test]
    fn test_region_trailing_ref_escape_rejection() {
        let code = r#"
        struct Point {
            x: i64,
            y: i64,
        }

        fn escape_trailing() {
            let escaped_ref = region r {
                let p = Point { x: 1, y: 2 };
                &p
            };
        }
        "#;

        let ast = parse(code).expect("syntax parse ok");
        let res = check(&ast);
        assert!(res.is_err(), "Yielding reference from region block should be rejected");
        let errs = res.unwrap_err();
        assert!(errs.iter().any(|e| e.message.contains("Region escape violation")));
    }

    #[test]
    fn test_resume_linearity_violation_rejection() {
        let code = r#"
        effect Db {
            fn query(sql: String) -> String;
        }

        fn run_query() -> String yields [Db] {
            Db::query("SELECT 1")
        }

        fn main() {
            handle {
                run_query();
            } with {
                Db::query(sql) => {
                    let r1 = resume("first");
                    let r2 = resume("second");
                    "done"
                }
            }
        }
        "#;

        let ast = parse(code).expect("syntax parse ok");
        let res = check(&ast);
        assert!(res.is_err(), "Multiple resume calls in an arm must be rejected");
        let errs = res.unwrap_err();
        assert!(errs.iter().any(|e| e.message.contains("Linearity violation")));
    }

    #[test]
    fn test_resume_outside_handler_rejection() {
        let code = r#"
        fn main() {
            resume(42);
        }
        "#;

        let ast = parse(code).expect("syntax parse ok");
        let res = check(&ast);
        assert!(res.is_err(), "Resume outside handler must be rejected");
        let errs = res.unwrap_err();
        assert!(errs.iter().any(|e| e.message.contains("Cannot call 'resume' outside of an effect handler arm")));
    }

    #[test]
    fn test_effect_row_polymorphism_and_unified_handlers() {
        let code = r#"
        effect Db {
            fn query(sql: String) -> String;
        }

        effect Logger {
            fn log(msg: String) -> ();
        }

        fn apply<T, U, E>(val: T, f: fn(T) yields [..E] -> U) -> U yields [..E] {
            f(val)
        }

        fn fetch_user(id: i64) -> String yields [Db, Logger] {
            Logger::log("fetching");
            Db::query("SELECT user")
        }

        fn main() {
            handle {
                let user = apply(42, fetch_user);
            } with {
                Db::query(sql) => {
                    resume("Alice")
                },
                Logger::log(msg) => {
                    resume(())
                }
            }
        }
        "#;

        let ast = parse(code).expect("syntax parse ok");
        let res = check(&ast);
        assert!(res.is_ok(), "Effect row polymorphism and unified handlers should succeed: {:?}", res.err());
    }
}

