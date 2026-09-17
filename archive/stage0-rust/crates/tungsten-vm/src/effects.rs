use std::collections::HashMap;
use tungsten_syntax::ast::HandlerArm;

#[derive(Debug, Clone)]
pub struct ActiveHandler {
    pub effect_name: String,
    pub arms: HashMap<String, HandlerArm>, // op_name -> Arm
}

impl ActiveHandler {
    pub fn new(effect_name: String) -> Self {
        Self {
            effect_name,
            arms: HashMap::new(),
        }
    }

    pub fn add_arm(&mut self, arm: HandlerArm) {
        self.arms.insert(arm.op_name.clone(), arm);
    }
}
