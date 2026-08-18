use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::TextViolation;
use crate::validation::validate_single_line_text;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct PathTemplate(String);

impl PathTemplate {
    pub fn new(value: impl Into<String>) -> Result<Self, TextViolation> {
        let template = Self(value.into());
        template.validate()?;
        Ok(template)
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn validate(&self) -> Result<(), TextViolation> {
        if self.0.is_empty() {
            return Err(TextViolation::Empty);
        }
        validate_single_line_text(&self.0)
    }

    pub fn expand(&self, context: &ExpansionContext) -> Result<PathBuf, PathExpansionError> {
        self.validate()
            .map_err(PathExpansionError::InvalidTemplate)?;
        let expanded = expand_variables(&self.0, &context.variables)?;
        let path = PathBuf::from(expanded);
        if path.is_absolute() {
            Ok(path)
        } else {
            Ok(context.base_directory.join(path))
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExpansionContext {
    base_directory: PathBuf,
    variables: BTreeMap<String, String>,
}

impl ExpansionContext {
    #[must_use]
    pub fn new(base_directory: impl Into<PathBuf>) -> Self {
        Self {
            base_directory: base_directory.into(),
            variables: BTreeMap::new(),
        }
    }

    #[must_use]
    pub fn with_variable(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.variables.insert(name.into(), value.into());
        self
    }

    #[must_use]
    pub fn base_directory(&self) -> &Path {
        &self.base_directory
    }
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum PathExpansionError {
    #[error("path template is invalid: {0}")]
    InvalidTemplate(TextViolation),
    #[error("path template contains an unterminated variable expression")]
    UnterminatedVariable,
    #[error("'{0}' is not a valid portable-path variable name")]
    InvalidVariableName(String),
    #[error("portable-path variable '{0}' is not defined")]
    MissingVariable(String),
}

fn expand_variables(
    template: &str,
    variables: &BTreeMap<String, String>,
) -> Result<String, PathExpansionError> {
    let mut output = String::with_capacity(template.len());
    let mut characters = template.char_indices().peekable();

    while let Some((_, character)) = characters.next() {
        if character != '$' {
            output.push(character);
            continue;
        }

        let Some(&(next_index, next_character)) = characters.peek() else {
            output.push('$');
            continue;
        };

        if next_character == '$' {
            characters.next();
            output.push('$');
            continue;
        }

        let variable_name = if next_character == '{' {
            characters.next();
            let mut name = String::new();
            let mut closed = false;
            for (_, value) in characters.by_ref() {
                if value == '}' {
                    closed = true;
                    break;
                }
                name.push(value);
            }
            if !closed {
                return Err(PathExpansionError::UnterminatedVariable);
            }
            name
        } else if is_variable_start(next_character) {
            let mut name = String::new();
            while let Some(&(_, value)) = characters.peek() {
                if !is_variable_continue(value) {
                    break;
                }
                name.push(value);
                characters.next();
            }
            name
        } else {
            let _ = next_index;
            output.push('$');
            continue;
        };

        if !is_valid_variable_name(&variable_name) {
            return Err(PathExpansionError::InvalidVariableName(variable_name));
        }
        let value = variables
            .get(&variable_name)
            .ok_or_else(|| PathExpansionError::MissingVariable(variable_name.clone()))?;
        output.push_str(value);
    }

    Ok(output)
}

const fn is_variable_start(character: char) -> bool {
    character == '_' || character.is_ascii_alphabetic()
}

const fn is_variable_continue(character: char) -> bool {
    character == '_' || character.is_ascii_alphanumeric()
}

fn is_valid_variable_name(name: &str) -> bool {
    let mut characters = name.chars();
    characters.next().is_some_and(is_variable_start) && characters.all(is_variable_continue)
}
