use std::{fmt::Debug, marker::PhantomData};
use facet::Facet;

#[derive(Facet, Clone, Debug, PartialEq)]
pub struct Event {
    pub id: u32, // server-assigned — not collected by any form field
    pub title: String,
    pub location: Location,
}

#[derive(Facet, Clone, Debug, PartialEq)]
pub struct Location {
    pub street: String,
    pub city: String,
    pub zip: String,
}

#[derive(Clone, Debug)]
pub struct Form<T: Clone + Debug + PartialEq> {
    pub title: Option<String>,
    pub members: Vec<Box<dyn FormMember>>,
    
    pub _type: PhantomData<T>,
}

impl<T: Clone + Debug + PartialEq> Form<T> {
    
}

pub trait FormMember: Debug {
    fn name(&self) -> String;
    fn label(&self) -> Option<String>;
    fn render(&self) -> String; // Element, later
    fn validate(&mut self);
    fn clone_box(&self) -> Box<dyn FormMember>;
}

#[derive(Clone, Debug, PartialEq)]
pub struct FormField<T: Clone + Debug + PartialEq> {
    pub name: String,
    pub label: Option<String>,
    pub value: FieldValue<T>,
    pub errors: Vec<FieldError>,
}

impl<T: Clone + Debug + PartialEq + 'static> FormMember for FormField<T> {
    fn name(&self) -> String { self.name.clone() }

    fn label(&self) -> Option<String> { self.label.clone() }

    fn render(&self) -> String {
        "todo".to_string()
    }

    fn validate(&mut self) {
        todo!()
    }

    fn clone_box(&self) -> Box<dyn FormMember> {
        Box::new(self.clone())
    }
}

#[derive(Clone, Debug)]
pub struct FieldSet<T: Clone + Debug + PartialEq> {
    pub name: String,
    pub label: Option<String>,
    pub members: Vec<Box<dyn FormMember>>,

    pub _type: PhantomData<T>,
}

impl<T: Clone + Debug + PartialEq + 'static> FormMember for FieldSet<T> {
    fn name(&self) -> String { self.name.clone() }

    fn label(&self) -> Option<String> { self.label.clone() }

    fn render(&self) -> String {
        "todo".to_string()
    }

    fn validate(&mut self) {
        todo!()
    }

    fn clone_box(&self) -> Box<dyn FormMember> {
        Box::new(self.clone())
    }
}

impl Clone for Box<dyn FormMember> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum FieldValue<T: Clone + Debug + PartialEq> {
    Empty,
    Valid(T),
    Invalid { raw: String, error: FieldError, },
}

#[derive(Clone, Debug, PartialEq)]
pub struct FieldError(pub String);

fn main() {
    println!("Hello, world!");
}
