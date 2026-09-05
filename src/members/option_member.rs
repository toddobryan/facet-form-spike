use std::collections::HashMap;

use crate::FormMember;


#[derive(Clone, Debug)]
pub struct OptionMember {
    pub inner: Box<dyn FormMember>,
}

impl FormMember for OptionMember {
    fn name(&self) -> String {
        self.inner.name()
    }

    fn label(&self) -> Option<String> {
        self.inner.label()
    }

    fn render(&self) -> String {
        self.inner.render()
    }

    fn raw_value(&self) -> String {
        self.inner.raw_value()
    }

    fn collect_leaves(&self, prefix: &str, out: &mut Vec<(String, String)>) {
        self.inner.collect_leaves(prefix, out);
    }

    fn apply_leaves(&mut self, prefix: &str, values: &HashMap<String, String>) {
        self.inner.apply_leaves(prefix, values);
    }

    fn validate(&mut self) {
        if self.is_present() {
            self.inner.validate()
        } else {
            self.inner.clear_errors()
        }
    }

    fn has_errors(&self) -> bool {
        self.inner.has_errors()
    }

    fn clone_box(&self) -> Box<dyn FormMember> {
        Box::new(self.clone())
    }
    
    fn write_value_into<'p>(&self, partial: facet::Partial<'p>) -> Result<facet::Partial<'p>, facet::ReflectError> {
        if self.is_present() {
            self.inner.write_value_into(partial.begin_some()?)?.end()
        } else {
            partial.set_default()
        }
    }
    
    fn is_present(&self) -> bool {
        self.inner.is_present()
    }

    fn clear_errors(&mut self) {
        self.inner.clear_errors();
    }
}
