//! A list-typed member. Rows are members named by their index, which is what
//! makes `answer_choices.0.text` fall out of the ordinary `qualify` nesting.

use facet::{Partial, ReflectError};
use std::collections::HashMap;
use crate::error::FormError;
use crate::members::{FormMember, qualify};

#[derive(Clone, Debug)]
pub struct ListSet {
    pub name: String,
    pub label: Option<String>,
    pub rows: Vec<Box<dyn FormMember>>,
    pub errors: Vec<FormError>,
}

impl FormMember for ListSet {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn label(&self) -> Option<String> {
        self.label.clone()
    }

    fn render(&self) -> String {
        self.rows
            .iter()
            .map(|r| r.render())
            .collect::<Vec<String>>()
            .join("\n")
    }

    fn raw_value(&self) -> String {
        String::new()
    }

    fn collect_leaves(&self, prefix: &str, out: &mut Vec<(String, String)>) {
        let nested = qualify(prefix, &self.name);
        for r in self.rows.iter() {
            r.collect_leaves(&nested, out);
        }
    }

    fn apply_leaves(&mut self, prefix: &str, values: &HashMap<String, String>) {
        let nested = qualify(prefix, &self.name);
        for r in self.rows.iter_mut() {
            r.apply_leaves(&nested, values);
        }
    }

    fn validate(&mut self) {
        self.errors.clear();
        for r in self.rows.iter_mut() {
            r.validate();
        }
    }

    fn has_errors(&self) -> bool {
        !self.errors.is_empty() || self.rows.iter().any(|r| r.has_errors())
    }

    fn clone_box(&self) -> Box<dyn FormMember> {
        Box::new(self.clone())
    }

    fn write_value_into<'p>(&self, partial: Partial<'p>) -> Result<Partial<'p>, ReflectError> {
        let mut partial = partial.init_list()?;
        for r in self.rows.iter() {
            partial = partial.begin_list_item()?;
            partial = r.write_value_into(partial)?;
            partial = partial.end()?;
        }
        Ok(partial)
    }
}
