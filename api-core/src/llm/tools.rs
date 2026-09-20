//! Tool definitions for function calling — OpenAI `tools` format.

use serde::{Deserialize, Serialize};

/// OpenAI-compatible tool definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function: FunctionSchema,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionSchema {
    pub name: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<JsonSchema>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonSchema {
    #[serde(rename = "type", default = "default_type")]
    pub type_field: String,
    pub properties: Option<serde_json::Map<String, serde_json::Value>>,
    #[serde(default)]
    pub required: Vec<String>,
}

fn default_type() -> String {
    "object".to_string()
}

impl ToolDefinition {
    pub fn new(name: &str, description: &str, parameters: JsonSchema) -> Self {
        Self {
            tool_type: "function".to_string(),
            function: FunctionSchema {
                name: name.to_string(),
                description: description.to_string(),
                parameters: Some(parameters),
            },
        }
    }
}
