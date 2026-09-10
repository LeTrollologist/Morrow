use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;

#[derive(Debug, Clone)]
pub enum Value {
    Int(i64),
    Str(String),
    Bool(bool),
    Unit,
    Struct {
        name: String,
        fields: HashMap<String, Value>,
    },
    Ref(Rc<RefCell<Value>>),
    Fn(String),
}

impl Value {
    pub fn as_int(&self) -> Option<i64> {
        match self {
            Value::Int(n) => Some(*n),
            Value::Ref(r) => r.borrow().as_int(),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<String> {
        match self {
            Value::Str(s) => Some(s.clone()),
            Value::Ref(r) => r.borrow().as_str(),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            Value::Ref(r) => r.borrow().as_bool(),
            _ => None,
        }
    }

    pub fn get_field(&self, field: &str) -> Option<Value> {
        match self {
            Value::Struct { fields, .. } => fields.get(field).cloned(),
            Value::Ref(r) => r.borrow().get_field(field),
            _ => None,
        }
    }

    pub fn set_field(&mut self, field: &str, new_val: Value) -> Result<(), String> {
        match self {
            Value::Struct { fields, .. } => {
                fields.insert(field.to_string(), new_val);
                Ok(())
            }
            Value::Ref(r) => r.borrow_mut().set_field(field, new_val),
            _ => Err(format!("Cannot set field '{}' on non-struct", field)),
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Int(n) => write!(f, "{}", n),
            Value::Str(s) => write!(f, "{}", s),
            Value::Bool(b) => write!(f, "{}", b),
            Value::Unit => write!(f, "()"),
            Value::Struct { name, fields } => {
                let field_strs: Vec<String> = fields
                    .iter()
                    .map(|(k, v)| format!("{}: {}", k, v))
                    .collect();
                write!(f, "{} {{ {} }}", name, field_strs.join(", "))
            }
            Value::Ref(r) => write!(f, "&{}", r.borrow()),
            Value::Fn(name) => write!(f, "<fn {}>", name),
        }
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Int(a), Value::Int(b)) => a == b,
            (Value::Str(a), Value::Str(b)) => a == b,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Unit, Value::Unit) => true,
            (Value::Fn(a), Value::Fn(b)) => a == b,
            (Value::Ref(a), b) => *a.borrow() == *b,
            (a, Value::Ref(b)) => *a == *b.borrow(),
            (Value::Struct { name: n1, fields: f1 }, Value::Struct { name: n2, fields: f2 }) => {
                n1 == n2 && f1 == f2
            }
            _ => false,
        }
    }
}
