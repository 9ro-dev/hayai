use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize)]
pub struct OpenApiSpec {
    pub openapi: String,
    pub info: Info,
    pub paths: HashMap<String, HashMap<String, Operation>>,
    #[serde(rename = "components")]
    pub schemas: HashMap<String, Schema>,
}

// Custom serialization for components wrapper
impl OpenApiSpec {
    pub fn to_json(&self) -> serde_json::Value {
        let mut val = serde_json::json!({
            "openapi": self.openapi,
            "info": {
                "title": self.info.title,
                "version": self.info.version,
            },
            "paths": {},
            "components": {
                "schemas": {}
            }
        });
        
        // Build paths
        let paths = val["paths"].as_object_mut().unwrap();
        for (path, methods) in &self.paths {
            let mut path_obj = serde_json::Map::new();
            for (method, op) in methods {
                path_obj.insert(method.clone(), serde_json::to_value(op).unwrap());
            }
            paths.insert(path.clone(), serde_json::Value::Object(path_obj));
        }
        
        // Build schemas
        let schemas = val["components"]["schemas"].as_object_mut().unwrap();
        for (name, schema) in &self.schemas {
            schemas.insert(name.clone(), serde_json::to_value(schema).unwrap());
        }
        
        val
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Info {
    pub title: String,
    pub version: String,
}

#[derive(Debug, Clone)]
pub struct Operation {
    pub summary: Option<String>,
    pub operation_id: Option<String>,
    pub parameters: Vec<Parameter>,
    pub request_body: Option<RequestBody>,
    pub responses: HashMap<String, ResponseDef>,
}

impl Serialize for Operation {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(None)?;
        if let Some(s) = &self.summary { map.serialize_entry("summary", s)?; }
        if let Some(s) = &self.operation_id { map.serialize_entry("operationId", s)?; }
        if !self.parameters.is_empty() { map.serialize_entry("parameters", &self.parameters)?; }
        if let Some(rb) = &self.request_body {
            map.serialize_entry("requestBody", &rb.to_json_value())?;
        }
        // Serialize responses with schema refs
        let mut resp = serde_json::Map::new();
        for (code, r) in &self.responses {
            let mut obj = serde_json::Map::new();
            obj.insert("description".into(), serde_json::Value::String(r.description.clone()));
            if let Some(schema_ref) = &r.schema_ref {
                let content = serde_json::json!({
                    "application/json": {
                        "schema": { "$ref": schema_ref }
                    }
                });
                obj.insert("content".into(), content);
            }
            resp.insert(code.clone(), serde_json::Value::Object(obj));
        }
        map.serialize_entry("responses", &resp)?;
        map.end()
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Parameter {
    pub name: &'static str,
    #[serde(rename = "in")]
    pub location: &'static str,
    pub required: bool,
    pub schema: SchemaObject,
}

#[derive(Debug, Clone, Serialize)]
pub struct SchemaObject {
    #[serde(rename = "type")]
    pub type_name: &'static str,
}

impl SchemaObject {
    pub const fn new_type(t: &'static str) -> Self {
        Self { type_name: t }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct RequestBody {
    pub required: bool,
    #[serde(skip)]
    pub content_type: String,
    #[serde(skip)]
    pub schema_ref: String,
}

// Custom serialize for RequestBody
impl RequestBody {
    pub fn to_json_value(&self) -> serde_json::Value {
        serde_json::json!({
            "required": self.required,
            "content": {
                &self.content_type: {
                    "schema": {
                        "$ref": &self.schema_ref
                    }
                }
            }
        })
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ResponseDef {
    pub description: String,
    #[serde(skip)]
    pub schema_ref: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Schema {
    #[serde(rename = "type")]
    pub type_name: String,
    pub properties: HashMap<String, Property>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub required: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Property {
    #[serde(rename = "type")]
    pub type_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    #[serde(rename = "minLength", skip_serializing_if = "Option::is_none")]
    pub min_length: Option<usize>,
    #[serde(rename = "maxLength", skip_serializing_if = "Option::is_none")]
    pub max_length: Option<usize>,
}

pub struct PropertyPatch {
    pub min_length: Option<usize>,
    pub max_length: Option<usize>,
    pub format: Option<String>,
}

/// Convert schemars schema to our OpenAPI schema
pub fn schema_from_schemars(_name: &str, root: &schemars::schema::RootSchema) -> Schema {
    let mut properties = HashMap::new();
    let mut required = Vec::new();
    
    if let Some(obj) = &root.schema.object {
        for (prop_name, prop_schema) in &obj.properties {
            let type_name = match prop_schema {
                schemars::schema::Schema::Object(obj) => {
                    if let Some(ty) = &obj.instance_type {
                        match ty {
                            schemars::schema::SingleOrVec::Single(single) => {
                                format_instance_type(single)
                            }
                            schemars::schema::SingleOrVec::Vec(vec) => {
                                if let Some(first) = vec.first() {
                                    format_instance_type(first)
                                } else {
                                    "string".to_string()
                                }
                            }
                        }
                    } else {
                        "string".to_string()
                    }
                }
                _ => "string".to_string(),
            };
            
            properties.insert(prop_name.clone(), Property {
                type_name,
                format: None,
                min_length: None,
                max_length: None,
            });
        }
        
        for req in &obj.required {
            required.push(req.clone());
        }
    }
    
    Schema {
        type_name: "object".to_string(),
        properties,
        required,
    }
}

fn format_instance_type(ty: &schemars::schema::InstanceType) -> String {
    match ty {
        schemars::schema::InstanceType::String => "string".to_string(),
        schemars::schema::InstanceType::Integer => "integer".to_string(),
        schemars::schema::InstanceType::Number => "number".to_string(),
        schemars::schema::InstanceType::Boolean => "boolean".to_string(),
        schemars::schema::InstanceType::Array => "array".to_string(),
        schemars::schema::InstanceType::Object => "object".to_string(),
        schemars::schema::InstanceType::Null => "null".to_string(),
    }
}
