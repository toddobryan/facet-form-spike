//! A struct-typed member: its fields, nested under its own name.

use facet::{Partial, ReflectError};
use std::collections::HashMap;
use crate::error::FormError;
use crate::members::{FormMember, qualify};

#[derive(Clone, Debug)]
pub struct FieldSet {
    pub name: String,
    pub label: Option<String>,
    pub members: Vec<Box<dyn FormMember>>,
    pub errors: Vec<FormError>,
}

impl FormMember for FieldSet {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn label(&self) -> Option<String> {
        self.label.clone()
    }

    fn render(&self) -> String {
        // TODO: need to decide whether to wrap in <fieldset> and whether we want to prefix
        // the names somehow, in case there are multiple fieldsets in the same form
        self.members
            .iter()
            .map(|m| m.render())
            .collect::<Vec<String>>()
            .join("\n")
    }

    fn validate(&mut self) {
        self.errors.clear();
        for m in self.members.iter_mut() {
            m.validate();
        }
    }

    fn clone_box(&self) -> Box<dyn FormMember> {
        Box::new(self.clone())
    }

    fn has_errors(&self) -> bool {
        !self.errors.is_empty() || self.members.iter().any(|m| m.has_errors())
    }

    fn raw_value(&self) -> String {
        // A field set isn't a scalar — it has no single input of its own.
        String::new()
    }

    fn collect_leaves(&self, prefix: &str, out: &mut Vec<(String, String)>) {
        let nested = qualify(prefix, &self.name);
        for m in self.members.iter() {
            m.collect_leaves(&nested, out);
        }
    }

    fn apply_leaves(&mut self, prefix: &str, values: &HashMap<String, String>) {
        let nested = qualify(prefix, &self.name);
        for m in self.members.iter_mut() {
            m.apply_leaves(&nested, values);
        }
    }

    fn write_value_into<'p>(&self, mut partial: Partial<'p>) -> Result<Partial<'p>, ReflectError> {
        for m in self.members.iter() {
            partial = m.write_into(partial)?;
        }
        Ok(partial)
    }
    
    fn is_present(&self) -> bool {
        self.members.iter().any(|fm| fm.is_present())
    }
    
    fn clear_errors(&mut self) {
        self.errors.clear();
        for m in self.members.iter_mut() {
            m.clear_errors();
        }
    }
}
