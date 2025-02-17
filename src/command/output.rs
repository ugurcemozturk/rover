use std::collections::BTreeMap;
use std::fmt::Debug;
use std::io;

use crate::command::supergraph::compose::CompositionOutput;
use crate::options::JsonVersion;
use crate::utils::table::{self, row};
use crate::RoverError;

use crate::options::GithubTemplate;
use atty::Stream;
use calm_io::{stderr, stderrln};
use camino::Utf8PathBuf;
use crossterm::style::Attribute::Underlined;
use rover_client::operations::contract::describe::ContractDescribeResponse;
use rover_client::operations::contract::publish::ContractPublishResponse;
use rover_client::operations::graph::publish::GraphPublishResponse;
use rover_client::operations::subgraph::delete::SubgraphDeleteResponse;
use rover_client::operations::subgraph::list::SubgraphListResponse;
use rover_client::operations::subgraph::publish::SubgraphPublishResponse;
use rover_client::shared::{
    CheckRequestSuccessResult, CheckResponse, FetchResponse, GraphRef, SdlType,
};
use rover_client::RoverClientError;
use rover_std::Style;
use serde_json::{json, Value};
use termimad::MadSkin;

/// RoverOutput defines all of the different types of data that are printed
/// to `stdout`. Every one of Rover's commands should return `saucer::Result<RoverOutput>`
/// If the command needs to output some type of data, it should be structured
/// in this enum, and its print logic should be handled in `RoverOutput::get_stdout`
///
/// Not all commands will output machine readable information, and those should
/// return `Ok(RoverOutput::EmptySuccess)`. If a new command is added and it needs to
/// return something that is not described well in this enum, it should be added.
#[derive(Clone, Eq, PartialEq, Debug)]
pub enum RoverOutput {
    ContractDescribe(ContractDescribeResponse),
    ContractPublish(ContractPublishResponse),
    DocsList(BTreeMap<&'static str, &'static str>),
    FetchResponse(FetchResponse),
    SupergraphSchema(String),
    CompositionResult(CompositionOutput),
    SubgraphList(SubgraphListResponse),
    CheckResponse(CheckResponse),
    AsyncCheckResponse(CheckRequestSuccessResult),
    GraphPublishResponse {
        graph_ref: GraphRef,
        publish_response: GraphPublishResponse,
    },
    SubgraphPublishResponse {
        graph_ref: GraphRef,
        subgraph: String,
        publish_response: SubgraphPublishResponse,
    },
    SubgraphDeleteResponse {
        graph_ref: GraphRef,
        subgraph: String,
        dry_run: bool,
        delete_response: SubgraphDeleteResponse,
    },
    TemplateList(Vec<GithubTemplate>),
    TemplateUseSuccess {
        template: GithubTemplate,
        path: Utf8PathBuf,
    },
    Profiles(Vec<String>),
    Introspection(String),
    ErrorExplanation(String),
    ReadmeFetchResponse {
        graph_ref: GraphRef,
        content: String,
        last_updated_time: Option<String>,
    },
    ReadmePublishResponse {
        graph_ref: GraphRef,
        new_content: String,
        last_updated_time: Option<String>,
    },
    EmptySuccess,
}

impl RoverOutput {
    pub fn get_stdout(&self) -> io::Result<Option<String>> {
        Ok(match self {
            RoverOutput::ContractDescribe(describe_response) => Some(format!(
                "{description}\nView the variant's full configuration at {variant_config}",
                description = &describe_response.description,
                variant_config = Style::Link.paint(format!(
                    "{}/graph/{}/settings/variant?variant={}",
                    describe_response.root_url,
                    describe_response.graph_ref.name,
                    describe_response.graph_ref.variant,
                ))
            )),
            RoverOutput::ContractPublish(publish_response) => {
                let launch_cli_copy = publish_response
                    .launch_cli_copy
                    .clone()
                    .unwrap_or_else(|| "No launch was triggered for this publish.".to_string());
                Some(format!(
                    "{description}\n{launch_cli_copy}",
                    description = &publish_response.config_description
                ))
            }
            RoverOutput::DocsList(shortlinks) => {
                stderrln!(
                    "You can open any of these documentation pages by running {}.\n",
                    Style::Command.paint("`rover docs open <slug>`")
                )?;
                let mut table = table::get_table();

                // bc => sets top row to be bold and center
                table.add_row(row![bc => "Slug", "Description"]);
                for (shortlink_slug, shortlink_description) in shortlinks {
                    table.add_row(row![shortlink_slug, shortlink_description]);
                }
                Some(format!("{}", table))
            }
            RoverOutput::FetchResponse(fetch_response) => {
                Some((fetch_response.sdl.contents).to_string())
            }
            RoverOutput::GraphPublishResponse {
                graph_ref,
                publish_response,
            } => {
                stderrln!(
                    "{}#{} published successfully {}",
                    graph_ref,
                    publish_response.api_schema_hash,
                    publish_response.change_summary
                )?;
                Some((publish_response.api_schema_hash).to_string())
            }
            RoverOutput::SubgraphPublishResponse {
                graph_ref,
                subgraph,
                publish_response,
            } => {
                if publish_response.subgraph_was_created {
                    stderrln!(
                        "A new subgraph called '{}' was created in '{}'",
                        subgraph,
                        graph_ref
                    )?;
                } else {
                    stderrln!("The '{}' subgraph in '{}' was updated", subgraph, graph_ref)?;
                }

                if publish_response.supergraph_was_updated {
                    stderrln!("The supergraph schema for '{}' was updated, composed from the updated '{}' subgraph", graph_ref, subgraph)?;
                } else {
                    stderrln!(
                        "The supergraph schema for '{}' was NOT updated with a new schema",
                        graph_ref
                    )?;
                }

                if let Some(launch_cli_copy) = &publish_response.launch_cli_copy {
                    stderrln!("{}", launch_cli_copy)?;
                }

                if !publish_response.build_errors.is_empty() {
                    let warn_prefix = Style::WarningPrefix.paint("WARN:");
                    stderrln!("{} The following build errors occurred:", warn_prefix)?;
                    stderrln!("{}", &publish_response.build_errors)?;
                }
                None
            }
            RoverOutput::SubgraphDeleteResponse {
                graph_ref,
                subgraph,
                dry_run,
                delete_response,
            } => {
                let warn_prefix = Style::WarningPrefix.paint("WARN:");
                if *dry_run {
                    if !delete_response.build_errors.is_empty() {
                        stderrln!(
                            "{} Deleting the {} subgraph from {} would result in the following build errors:",
                            warn_prefix,
                            Style::Link.paint(subgraph),
                            Style::Link.paint(graph_ref.to_string()),
                        )?;

                        stderrln!("{}", &delete_response.build_errors)?;
                        stderrln!("{} This is only a prediction. If the graph changes before confirming, these errors could change.", warn_prefix)?;
                    } else {
                        stderrln!("{} At the time of checking, there would be no build errors resulting from the deletion of this subgraph.", warn_prefix)?;
                        stderrln!("{} This is only a prediction. If the graph changes before confirming, there could be build errors.", warn_prefix)?;
                    }
                    None
                } else {
                    if delete_response.supergraph_was_updated {
                        stderrln!(
                            "The '{}' subgraph was removed from '{}'. The remaining subgraphs were composed.",
                            Style::Link.paint(subgraph),
                            Style::Link.paint(graph_ref.to_string()),
                        )?;
                    } else {
                        stderrln!(
                            "{} The supergraph schema for '{}' was not updated. See errors below.",
                            warn_prefix,
                            Style::Link.paint(graph_ref.to_string())
                        )?;
                    }

                    if !delete_response.build_errors.is_empty() {
                        stderrln!(
                            "{} There were build errors as a result of deleting the '{}' subgraph from '{}':",
                            warn_prefix,
                            Style::Link.paint(subgraph),
                            Style::Link.paint(graph_ref.to_string())
                        )?;

                        stderrln!("{}", &delete_response.build_errors)?;
                    }
                    None
                }
            }
            RoverOutput::SupergraphSchema(csdl) => Some((csdl).to_string()),
            RoverOutput::CompositionResult(composition_output) => {
                let warn_prefix = Style::HintPrefix.paint("HINT:");

                let hints_string = composition_output
                    .hints
                    .iter()
                    .map(|hint| format!("{} {}\n", warn_prefix, hint.message))
                    .collect::<String>();

                stderrln!("{}", hints_string)?;

                Some((composition_output.supergraph_sdl).to_string())
            }
            RoverOutput::SubgraphList(details) => {
                let mut table = table::get_table();

                // bc => sets top row to be bold and center
                table.add_row(row![bc => "Name", "Routing Url", "Last Updated"]);

                for subgraph in &details.subgraphs {
                    // Default to "unspecified" if the url is None or empty.
                    let url = subgraph
                        .url
                        .clone()
                        .unwrap_or_else(|| "unspecified".to_string());
                    let url = if url.is_empty() {
                        "unspecified".to_string()
                    } else {
                        url
                    };
                    let formatted_updated_at: String = if let Some(dt) = subgraph.updated_at.local {
                        dt.format("%Y-%m-%d %H:%M:%S %Z").to_string()
                    } else {
                        "N/A".to_string()
                    };

                    table.add_row(row![subgraph.name, url, formatted_updated_at]);
                }
                Some(format!(
                    "{}/n View full details at {}/graph/{}/service-list",
                    table, details.root_url, details.graph_ref.name
                ))
            }
            RoverOutput::TemplateList(templates) => {
                let mut table = table::get_table();

                // bc => sets top row to be bold and center
                table.add_row(row![bc => "Name", "ID", "Language", "Repo URL"]);

                for template in templates {
                    table.add_row(row![
                        template.display,
                        template.id,
                        template.language,
                        template.git_url
                    ]);
                }

                Some(format!("{}", table))
            }
            RoverOutput::TemplateUseSuccess { template, path } => {
                let template_id = Style::Command.paint(template.id);
                let path = Style::Path.paint(path.as_str());
                let readme = Style::Path.paint("README.md");
                let forum_call_to_action = Style::CallToAction.paint(
                    "Have a question or suggestion about templates? Let us know at \
                    https://community.apollographql.com",
                );
                Some(format!("Successfully created a new project from the '{}' template in {}/n Read the generated '{}' file for next steps./n{}",
                template_id,
                path,
                readme,
                forum_call_to_action))
            }
            RoverOutput::CheckResponse(check_response) => Some(check_response.get_table()),
            RoverOutput::AsyncCheckResponse(check_response) => Some(format!(
                "Check successfully started with workflow ID: {}/nView full details at {}",
                check_response.workflow_id, check_response.target_url
            )),
            RoverOutput::Profiles(profiles) => {
                if profiles.is_empty() {
                    stderrln!("No profiles found.")?;
                }
                Some(profiles.join("\n"))
            }
            RoverOutput::Introspection(introspection_response) => {
                Some((introspection_response).to_string())
            }
            RoverOutput::ErrorExplanation(explanation) => {
                // underline bolded md
                let mut skin = MadSkin::default();
                skin.bold.add_attr(Underlined);

                Some(format!("{}", skin.inline(explanation)))
            }
            RoverOutput::ReadmeFetchResponse {
                graph_ref: _,
                content,
                last_updated_time: _,
            } => Some((content).to_string()),
            RoverOutput::ReadmePublishResponse {
                graph_ref,
                new_content: _,
                last_updated_time: _,
            } => {
                stderrln!("Readme for {} published successfully", graph_ref,)?;
                None
            }
            RoverOutput::EmptySuccess => None,
        })
    }

    pub(crate) fn get_internal_data_json(&self) -> Value {
        match self {
            RoverOutput::ContractDescribe(describe_response) => json!(describe_response),
            RoverOutput::ContractPublish(publish_response) => json!(publish_response),
            RoverOutput::DocsList(shortlinks) => {
                let mut shortlink_vec = Vec::with_capacity(shortlinks.len());
                for (shortlink_slug, shortlink_description) in shortlinks {
                    shortlink_vec.push(
                        json!({"slug": shortlink_slug, "description": shortlink_description }),
                    );
                }
                json!({ "shortlinks": shortlink_vec })
            }
            RoverOutput::FetchResponse(fetch_response) => json!(fetch_response),
            RoverOutput::SupergraphSchema(csdl) => json!({ "core_schema": csdl }),
            RoverOutput::CompositionResult(composition_output) => {
                if let Some(federation_version) = &composition_output.federation_version {
                    json!({
                      "core_schema": composition_output.supergraph_sdl,
                      "hints": composition_output.hints,
                      "federation_version": federation_version
                    })
                } else {
                    json!({
                        "core_schema": composition_output.supergraph_sdl,
                        "hints": composition_output.hints
                    })
                }
            }
            RoverOutput::GraphPublishResponse {
                graph_ref: _,
                publish_response,
            } => json!(publish_response),
            RoverOutput::SubgraphPublishResponse {
                graph_ref: _,
                subgraph: _,
                publish_response,
            } => json!(publish_response),
            RoverOutput::SubgraphDeleteResponse {
                graph_ref: _,
                subgraph: _,
                dry_run: _,
                delete_response,
            } => {
                json!(delete_response)
            }
            RoverOutput::SubgraphList(list_response) => json!(list_response),
            RoverOutput::TemplateList(templates) => json!({ "templates": templates }),
            RoverOutput::TemplateUseSuccess { template, path } => {
                json!({ "template_id": template.id, "path": path })
            }
            RoverOutput::CheckResponse(check_response) => check_response.get_json(),
            RoverOutput::AsyncCheckResponse(check_response) => check_response.get_json(),
            RoverOutput::Profiles(profiles) => json!({ "profiles": profiles }),
            RoverOutput::Introspection(introspection_response) => {
                json!({ "introspection_response": introspection_response })
            }
            RoverOutput::ErrorExplanation(explanation_markdown) => {
                json!({ "explanation_markdown": explanation_markdown })
            }
            RoverOutput::ReadmeFetchResponse {
                graph_ref: _,
                content,
                last_updated_time,
            } => {
                json!({ "readme": content, "last_updated_time": last_updated_time})
            }
            RoverOutput::ReadmePublishResponse {
                graph_ref: _,
                new_content,
                last_updated_time,
            } => {
                json!({ "readme": new_content, "last_updated_time": last_updated_time })
            }
            RoverOutput::EmptySuccess => json!(null),
        }
    }

    pub(crate) fn get_internal_error_json(&self) -> Value {
        let rover_error = match self {
            RoverOutput::SubgraphPublishResponse {
                graph_ref,
                subgraph,
                publish_response,
            } => {
                if !publish_response.build_errors.is_empty() {
                    Some(RoverError::from(RoverClientError::SubgraphBuildErrors {
                        subgraph: subgraph.clone(),
                        graph_ref: graph_ref.clone(),
                        source: publish_response.build_errors.clone(),
                    }))
                } else {
                    None
                }
            }
            RoverOutput::SubgraphDeleteResponse {
                graph_ref,
                subgraph,
                dry_run: _,
                delete_response,
            } => {
                if !delete_response.build_errors.is_empty() {
                    Some(RoverError::from(RoverClientError::SubgraphBuildErrors {
                        subgraph: subgraph.clone(),
                        graph_ref: graph_ref.clone(),
                        source: delete_response.build_errors.clone(),
                    }))
                } else {
                    None
                }
            }
            _ => None,
        };
        json!(rover_error)
    }

    pub(crate) fn get_json_version(&self) -> JsonVersion {
        JsonVersion::default()
    }

    pub(crate) fn print_descriptor(&self) -> io::Result<()> {
        if atty::is(Stream::Stdout) {
            if let Some(descriptor) = self.descriptor() {
                stderrln!("{}: \n", Style::Heading.paint(descriptor))?;
            }
        }
        Ok(())
    }
    pub(crate) fn print_one_line_descriptor(&self) -> io::Result<()> {
        if atty::is(Stream::Stdout) {
            if let Some(descriptor) = self.descriptor() {
                stderr!("{}: ", Style::Heading.paint(descriptor))?;
            }
        }
        Ok(())
    }
    pub(crate) fn descriptor(&self) -> Option<&str> {
        match &self {
            RoverOutput::ContractDescribe(_) => Some("Configuration Description"),
            RoverOutput::ContractPublish(_) => Some("New Configuration Description"),
            RoverOutput::FetchResponse(fetch_response) => match fetch_response.sdl.r#type {
                SdlType::Graph | SdlType::Subgraph { .. } => Some("Schema"),
                SdlType::Supergraph => Some("Supergraph Schema"),
            },
            RoverOutput::CompositionResult(_) | RoverOutput::SupergraphSchema(_) => {
                Some("Supergraph Schema")
            }
            RoverOutput::TemplateUseSuccess { .. } => Some("Project generated"),
            RoverOutput::CheckResponse(_) => Some("Check Result"),
            RoverOutput::AsyncCheckResponse(_) => Some("Check Started"),
            RoverOutput::Profiles(_) => Some("Profiles"),
            RoverOutput::Introspection(_) => Some("Introspection Response"),
            RoverOutput::ReadmeFetchResponse { .. } => Some("Readme"),
            RoverOutput::GraphPublishResponse { .. } => Some("Schema Hash"),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use assert_json_diff::assert_json_eq;
    use chrono::{DateTime, Local, Utc};
    use rover_client::{
        operations::{
            graph::publish::{ChangeSummary, FieldChanges, TypeChanges},
            subgraph::{
                delete::SubgraphDeleteResponse,
                list::{SubgraphInfo, SubgraphUpdatedAt},
            },
        },
        shared::{ChangeSeverity, SchemaChange, Sdl, SdlType},
    };

    use apollo_federation_types::build::{BuildError, BuildErrors};

    use anyhow::anyhow;

    use crate::options::JsonOutput;

    use super::*;

    #[test]
    fn docs_list_json() {
        let mut mock_shortlinks = BTreeMap::new();
        mock_shortlinks.insert("slug_one", "description_one");
        mock_shortlinks.insert("slug_two", "description_two");
        let actual_json: JsonOutput = RoverOutput::DocsList(mock_shortlinks).into();
        let expected_json = json!(
        {
            "json_version": "1",
            "data": {
                "shortlinks": [
                    {
                        "slug": "slug_one",
                        "description": "description_one"
                    },
                    {
                        "slug": "slug_two",
                        "description": "description_two"
                    }
                ],
                "success": true
            },
            "error": null
        });
        assert_json_eq!(expected_json, actual_json);
    }

    #[test]
    fn fetch_response_json() {
        let mock_fetch_response = FetchResponse {
            sdl: Sdl {
                contents: "sdl contents".to_string(),
                r#type: SdlType::Subgraph {
                    routing_url: Some("http://localhost:8000/graphql".to_string()),
                },
            },
        };
        let actual_json: JsonOutput = RoverOutput::FetchResponse(mock_fetch_response).into();
        let expected_json = json!(
        {
            "json_version": "1",
            "data": {
                "sdl": {
                    "contents": "sdl contents",
                },
                "success": true
            },
            "error": null
        });
        assert_json_eq!(expected_json, actual_json);
    }

    #[test]
    fn core_schema_json() {
        let mock_core_schema = "core schema contents".to_string();
        let actual_json: JsonOutput = RoverOutput::SupergraphSchema(mock_core_schema).into();
        let expected_json = json!(
        {
            "json_version": "1",
            "data": {
                "core_schema": "core schema contents",
                "success": true
            },
            "error": null
        });
        assert_json_eq!(expected_json, actual_json);
    }

    #[test]
    fn subgraph_list_json() {
        let now_utc: DateTime<Utc> = Utc::now();
        let now_local: DateTime<Local> = now_utc.into();
        let mock_subgraph_list_response = SubgraphListResponse {
            subgraphs: vec![
                SubgraphInfo {
                    name: "subgraph one".to_string(),
                    url: Some("http://localhost:4001".to_string()),
                    updated_at: SubgraphUpdatedAt {
                        local: Some(now_local),
                        utc: Some(now_utc),
                    },
                },
                SubgraphInfo {
                    name: "subgraph two".to_string(),
                    url: None,
                    updated_at: SubgraphUpdatedAt {
                        local: None,
                        utc: None,
                    },
                },
            ],
            root_url: "https://studio.apollographql.com/".to_string(),
            graph_ref: GraphRef {
                name: "graph".to_string(),
                variant: "current".to_string(),
            },
        };
        let actual_json: JsonOutput = RoverOutput::SubgraphList(mock_subgraph_list_response).into();
        let expected_json = json!(
        {
            "json_version": "1",
            "data": {
                "subgraphs": [
                    {
                        "name": "subgraph one",
                        "url": "http://localhost:4001",
                        "updated_at": {
                            "local": now_local,
                            "utc": now_utc
                        }
                    },
                    {
                        "name": "subgraph two",
                        "url": null,
                        "updated_at": {
                            "local": null,
                            "utc": null
                        }
                    }
                ],
                "success": true
          },
          "error": null
        });
        assert_json_eq!(expected_json, actual_json);
    }

    #[test]
    fn subgraph_delete_success_json() {
        let mock_subgraph_delete = SubgraphDeleteResponse {
            supergraph_was_updated: true,
            build_errors: BuildErrors::new(),
        };
        let actual_json: JsonOutput = RoverOutput::SubgraphDeleteResponse {
            delete_response: mock_subgraph_delete,
            subgraph: "subgraph".to_string(),
            dry_run: false,
            graph_ref: GraphRef {
                name: "name".to_string(),
                variant: "current".to_string(),
            },
        }
        .into();
        let expected_json = json!(
        {
            "json_version": "1",
            "data": {
                "supergraph_was_updated": true,
                "success": true,
            },
            "error": null
        });
        assert_json_eq!(expected_json, actual_json);
    }

    #[test]
    fn subgraph_delete_build_errors_json() {
        let mock_subgraph_delete = SubgraphDeleteResponse {
            supergraph_was_updated: false,
            build_errors: vec![
                BuildError::composition_error(
                    Some("AN_ERROR_CODE".to_string()),
                    Some("[Accounts] -> Things went really wrong".to_string()),
                ),
                BuildError::composition_error(
                    None,
                    Some("[Films] -> Something else also went wrong".to_string()),
                ),
            ]
            .into(),
        };
        let actual_json: JsonOutput = RoverOutput::SubgraphDeleteResponse {
            delete_response: mock_subgraph_delete,
            subgraph: "subgraph".to_string(),
            dry_run: true,
            graph_ref: GraphRef {
                name: "name".to_string(),
                variant: "current".to_string(),
            },
        }
        .into();
        let expected_json = json!(
        {
            "json_version": "1",
            "data": {
                "supergraph_was_updated": false,
                "success": true,
            },
            "error": {
                "message": "Encountered 2 build errors while trying to build subgraph \"subgraph\" into supergraph \"name@current\".",
                "code": "E029",
                "details": {
                    "build_errors": [
                        {
                            "message": "[Accounts] -> Things went really wrong",
                            "code": "AN_ERROR_CODE",
                            "type": "composition"
                        },
                        {
                            "message": "[Films] -> Something else also went wrong",
                            "code": null,
                            "type": "composition"
                        }
                    ],
                }
            }
        });
        assert_json_eq!(expected_json, actual_json);
    }

    #[test]
    fn supergraph_fetch_no_successful_publishes_json() {
        let graph_ref = GraphRef {
            name: "name".to_string(),
            variant: "current".to_string(),
        };
        let source = BuildErrors::from(vec![
            BuildError::composition_error(
                Some("AN_ERROR_CODE".to_string()),
                Some("[Accounts] -> Things went really wrong".to_string()),
            ),
            BuildError::composition_error(
                None,
                Some("[Films] -> Something else also went wrong".to_string()),
            ),
        ]);
        let actual_json: JsonOutput =
            RoverError::new(RoverClientError::NoSupergraphBuilds { graph_ref, source }).into();
        let expected_json = json!(
        {
            "json_version": "1",
            "data": {
                "success": false
            },
            "error": {
                "message": "No supergraph SDL exists for \"name@current\" because its subgraphs failed to build.",
                "details": {
                    "build_errors": [
                        {
                            "message": "[Accounts] -> Things went really wrong",
                            "code": "AN_ERROR_CODE",
                            "type": "composition",
                        },
                        {
                            "message": "[Films] -> Something else also went wrong",
                            "code": null,
                            "type": "composition"
                        }
                    ]
                },
                "code": "E027"
            }
        });
        assert_json_eq!(actual_json, expected_json);
    }

    #[test]
    fn check_success_response_json() {
        let graph_ref = GraphRef {
            name: "name".to_string(),
            variant: "current".to_string(),
        };
        let mock_check_response = CheckResponse::try_new(
            Some("https://studio.apollographql.com/graph/my-graph/composition/big-hash?variant=current".to_string()),
            10,
            vec![
                SchemaChange {
                    code: "SOMETHING_HAPPENED".to_string(),
                    description: "beeg yoshi".to_string(),
                    severity: ChangeSeverity::PASS,
                },
                SchemaChange {
                    code: "WOW".to_string(),
                    description: "that was so cool".to_string(),
                    severity: ChangeSeverity::PASS,
                }
            ],
            ChangeSeverity::PASS,
            graph_ref,
            true,
        );
        if let Ok(mock_check_response) = mock_check_response {
            let actual_json: JsonOutput = RoverOutput::CheckResponse(mock_check_response).into();
            let expected_json = json!(
            {
                "json_version": "1",
                "data": {
                    "target_url": "https://studio.apollographql.com/graph/my-graph/composition/big-hash?variant=current",
                    "operation_check_count": 10,
                    "changes": [
                        {
                            "code": "SOMETHING_HAPPENED",
                            "description": "beeg yoshi",
                            "severity": "PASS"
                        },
                        {
                            "code": "WOW",
                            "description": "that was so cool",
                            "severity": "PASS"
                        },
                    ],
                    "failure_count": 0,
                    "success": true,
                    "core_schema_modified": true,
                },
                "error": null
            });
            assert_json_eq!(expected_json, actual_json);
        } else {
            panic!("The shape of this response should return a CheckResponse")
        }
    }

    #[test]
    fn check_failure_response_json() {
        let graph_ref = GraphRef {
            name: "name".to_string(),
            variant: "current".to_string(),
        };
        let check_response = CheckResponse::try_new(
            Some("https://studio.apollographql.com/graph/my-graph/composition/big-hash?variant=current".to_string()),
            10,
            vec![
                SchemaChange {
                    code: "SOMETHING_HAPPENED".to_string(),
                    description: "beeg yoshi".to_string(),
                    severity: ChangeSeverity::FAIL,
                },
                SchemaChange {
                    code: "WOW".to_string(),
                    description: "that was so cool".to_string(),
                    severity: ChangeSeverity::FAIL,
                }
            ],
            ChangeSeverity::FAIL, graph_ref,
            false,
        );

        if let Err(operation_check_failure) = check_response {
            let actual_json: JsonOutput = RoverError::new(operation_check_failure).into();
            let expected_json = json!(
            {
                "json_version": "1",
                "data": {
                    "target_url": "https://studio.apollographql.com/graph/my-graph/composition/big-hash?variant=current",
                    "operation_check_count": 10,
                    "changes": [
                        {
                            "code": "SOMETHING_HAPPENED",
                            "description": "beeg yoshi",
                            "severity": "FAIL"
                        },
                        {
                            "code": "WOW",
                            "description": "that was so cool",
                            "severity": "FAIL"
                        },
                    ],
                    "failure_count": 2,
                    "success": false,
                    "core_schema_modified": false,
                },
                "error": {
                    "message": "This operation check has encountered 2 schema changes that would break operations from existing client traffic.",
                    "code": "E030",
                }
            });
            assert_json_eq!(expected_json, actual_json);
        } else {
            panic!("The shape of this response should return a RoverClientError")
        }
    }

    #[test]
    fn graph_publish_response_json() {
        let mock_publish_response = GraphPublishResponse {
            api_schema_hash: "123456".to_string(),
            change_summary: ChangeSummary {
                field_changes: FieldChanges {
                    additions: 2,
                    removals: 1,
                    edits: 0,
                },
                type_changes: TypeChanges {
                    additions: 4,
                    removals: 0,
                    edits: 7,
                },
            },
        };
        let actual_json: JsonOutput = RoverOutput::GraphPublishResponse {
            graph_ref: GraphRef {
                name: "graph".to_string(),
                variant: "variant".to_string(),
            },
            publish_response: mock_publish_response,
        }
        .into();
        let expected_json = json!(
        {
            "json_version": "1",
            "data": {
                "api_schema_hash": "123456",
                "field_changes": {
                    "additions": 2,
                    "removals": 1,
                    "edits": 0
                },
                "type_changes": {
                    "additions": 4,
                    "removals": 0,
                    "edits": 7
                },
                "success": true
            },
            "error": null
        });
        assert_json_eq!(expected_json, actual_json);
    }

    #[test]
    fn subgraph_publish_success_response_json() {
        let mock_publish_response = SubgraphPublishResponse {
            api_schema_hash: Some("123456".to_string()),
            build_errors: BuildErrors::new(),
            supergraph_was_updated: true,
            subgraph_was_created: true,
            launch_url: Some("test.com/launchurl".to_string()),
            launch_cli_copy: Some(
                "You can monitor this launch in Apollo Studio: test.com/launchurl".to_string(),
            ),
        };
        let actual_json: JsonOutput = RoverOutput::SubgraphPublishResponse {
            graph_ref: GraphRef {
                name: "graph".to_string(),
                variant: "variant".to_string(),
            },
            subgraph: "subgraph".to_string(),
            publish_response: mock_publish_response,
        }
        .into();
        let expected_json = json!(
        {
            "json_version": "1",
            "data": {
                "api_schema_hash": "123456",
                "supergraph_was_updated": true,
                "subgraph_was_created": true,
                "success": true,
                "launch_url": "test.com/launchurl",
                "launch_cli_copy": "You can monitor this launch in Apollo Studio: test.com/launchurl",
            },
            "error": null
        });
        assert_json_eq!(expected_json, actual_json);
    }

    #[test]
    fn subgraph_publish_failure_response_json() {
        let mock_publish_response = SubgraphPublishResponse {
            api_schema_hash: None,

            build_errors: vec![
                BuildError::composition_error(
                    Some("AN_ERROR_CODE".to_string()),
                    Some("[Accounts] -> Things went really wrong".to_string()),
                ),
                BuildError::composition_error(
                    None,
                    Some("[Films] -> Something else also went wrong".to_string()),
                ),
            ]
            .into(),
            supergraph_was_updated: false,
            subgraph_was_created: false,
            launch_url: None,
            launch_cli_copy: None,
        };
        let actual_json: JsonOutput = RoverOutput::SubgraphPublishResponse {
            graph_ref: GraphRef {
                name: "name".to_string(),
                variant: "current".to_string(),
            },
            subgraph: "subgraph".to_string(),
            publish_response: mock_publish_response,
        }
        .into();
        let expected_json = json!(
        {
            "json_version": "1",
            "data": {
                "api_schema_hash": null,
                "subgraph_was_created": false,
                "supergraph_was_updated": false,
                "success": true,
                "launch_url": null,
                "launch_cli_copy": null,
            },
            "error": {
                "message": "Encountered 2 build errors while trying to build subgraph \"subgraph\" into supergraph \"name@current\".",
                "code": "E029",
                "details": {
                    "build_errors": [
                        {
                            "message": "[Accounts] -> Things went really wrong",
                            "code": "AN_ERROR_CODE",
                            "type": "composition",
                        },
                        {
                            "message": "[Films] -> Something else also went wrong",
                            "code": null,
                            "type": "composition"
                        }
                    ]
                }
            }
        });
        assert_json_eq!(expected_json, actual_json);
    }

    #[test]
    fn profiles_json() {
        let mock_profiles = vec!["default".to_string(), "staging".to_string()];
        let actual_json: JsonOutput = RoverOutput::Profiles(mock_profiles).into();
        let expected_json = json!(
        {
            "json_version": "1",
            "data": {
                "profiles": [
                    "default",
                    "staging"
                ],
                "success": true
            },
            "error": null
        });
        assert_json_eq!(expected_json, actual_json);
    }

    #[test]
    fn introspection_json() {
        let actual_json: JsonOutput = RoverOutput::Introspection(
            "i cant believe its not a real introspection response".to_string(),
        )
        .into();
        let expected_json = json!(
        {
            "json_version": "1",
            "data": {
                "introspection_response": "i cant believe its not a real introspection response",
                "success": true
            },
            "error": null
        });
        assert_json_eq!(expected_json, actual_json);
    }

    #[test]
    fn error_explanation_json() {
        let actual_json: JsonOutput = RoverOutput::ErrorExplanation(
            "this error occurs when stuff is real complicated... I wouldn't worry about it"
                .to_string(),
        )
        .into();
        let expected_json = json!(
        {
            "json_version": "1",
            "data": {
                "explanation_markdown": "this error occurs when stuff is real complicated... I wouldn't worry about it",
                "success": true
            },
            "error": null
        }

        );
        assert_json_eq!(expected_json, actual_json);
    }

    #[test]
    fn empty_success_json() {
        let actual_json: JsonOutput = RoverOutput::EmptySuccess.into();
        let expected_json = json!(
        {
            "json_version": "1",
            "data": {
               "success": true
            },
            "error": null
        });
        assert_json_eq!(expected_json, actual_json);
    }

    #[test]
    fn base_error_message_json() {
        let actual_json: JsonOutput = RoverError::new(anyhow!("Some random error")).into();
        let expected_json = json!(
        {
            "json_version": "1",
            "data": {
                "success": false
            },
            "error": {
                "message": "Some random error",
                "code": null
            }
        });
        assert_json_eq!(expected_json, actual_json);
    }

    #[test]
    fn coded_error_message_json() {
        let actual_json: JsonOutput = RoverError::new(RoverClientError::NoSubgraphInGraph {
            invalid_subgraph: "invalid_subgraph".to_string(),
            valid_subgraphs: Vec::new(),
        })
        .into();
        let expected_json = json!(
        {
            "json_version": "1",
            "data": {
                "success": false
            },
            "error": {
                "message": "Could not find subgraph \"invalid_subgraph\".",
                "code": "E009"
            }
        });
        assert_json_eq!(expected_json, actual_json)
    }

    #[test]
    fn composition_error_message_json() {
        let source = BuildErrors::from(vec![
            BuildError::composition_error(
                Some("AN_ERROR_CODE".to_string()),
                Some("[Accounts] -> Things went really wrong".to_string()),
            ),
            BuildError::composition_error(
                None,
                Some("[Films] -> Something else also went wrong".to_string()),
            ),
        ]);
        let actual_json: JsonOutput = RoverError::from(RoverClientError::BuildErrors {
            source,
            num_subgraphs: 2,
        })
        .into();
        let expected_json = json!(
        {
            "json_version": "1",
            "data": {
                "success": false
            },
            "error": {
                "details": {
                    "build_errors": [
                        {
                            "message": "[Accounts] -> Things went really wrong",
                            "code": "AN_ERROR_CODE",
                            "type": "composition"
                        },
                        {
                            "message": "[Films] -> Something else also went wrong",
                            "code": null,
                            "type": "composition"
                        }
                    ],
                },
                "message": "Encountered 2 build errors while trying to build a supergraph.",
                "code": "E029"
            }
        });
        assert_json_eq!(expected_json, actual_json)
    }
}
