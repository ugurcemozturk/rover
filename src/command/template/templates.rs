use console::Term;
use dialoguer::Select;
use graphql_client::{GraphQLQuery, Response};
use reqwest::blocking::Client;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::env;

use crate::anyhow;
use crate::error::RoverError;
use crate::options::ProjectLanguage;
use crate::Result;

use super::queries::{
    get_template_by_id::GetTemplateByIdTemplate,
    get_templates_for_language::GetTemplatesForLanguageTemplates,
    list_templates_for_language::ListTemplatesForLanguageTemplates, *,
};

fn request<Body: Serialize, Data: DeserializeOwned>(body: &Body) -> Result<Data> {
    let uri = env::var("APOLLO_TEMPLATES_API")
        .unwrap_or_else(|_| "https://apollo-templates.up.railway.app".to_string());
    let resp = Client::new()
        .post(uri)
        .json(body)
        .send()
        .map_err(|e| anyhow!("Could not reach templates server: {}", e))?;
    let response: Response<Data> = resp
        .json()
        .map_err(|e| anyhow!("Could not parse response from templates server: {}", e))?;
    response
        .data
        .ok_or_else(|| anyhow!("No data in response from templates server").into())
}

/// Get a template by ID
pub fn get_template(template_id: &str) -> Result<Option<GetTemplateByIdTemplate>> {
    use super::queries::get_template_by_id::*;
    let query = GetTemplateById::build_query(Variables {
        id: template_id.to_string(),
    });
    let resp: ResponseData = request(&query)?;
    Ok(resp.template)
}

pub fn get_templates_for_language(
    language: ProjectLanguage,
) -> Result<Vec<GetTemplatesForLanguageTemplates>> {
    use super::queries::get_templates_for_language::*;
    let query = GetTemplatesForLanguage::build_query(Variables {
        language: Some(language.into()),
    });
    let resp: ResponseData = request(&query)?;
    error_if_empty(resp.templates)
}

pub fn list_templates(
    language: Option<ProjectLanguage>,
) -> Result<Vec<ListTemplatesForLanguageTemplates>> {
    use super::queries::list_templates_for_language::*;
    let query = ListTemplatesForLanguage::build_query(Variables {
        language: language.map(Into::into),
    });
    let resp: ResponseData = request(&query)?;
    error_if_empty(resp.templates)
}

pub fn error_if_empty<T>(values: Vec<T>) -> Result<Vec<T>> {
    if values.is_empty() {
        Err(RoverError::new(anyhow!(
            "No templates matched the provided filters"
        )))
    } else {
        Ok(values)
    }
}

/// Prompt to select a template
pub fn selection_prompt(
    mut templates: Vec<GetTemplatesForLanguageTemplates>,
) -> Result<GetTemplatesForLanguageTemplates> {
    let names = templates
        .iter()
        .map(|t| t.name.as_str())
        .collect::<Vec<_>>();
    let selection = Select::new()
        .with_prompt("Which template would you like to use?")
        .items(&names)
        .default(0)
        .interact_on_opt(&Term::stderr())?;

    match selection {
        Some(index) => Ok(templates.remove(index)),
        None => Err(RoverError::new(anyhow!("No template selected"))),
    }
}
