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
        if let Some(paths) = val["paths"].as_object_mut() {
            for (path, methods) in &self.paths {
                let mut path_obj = serde_json::Map::new();
                for (method, op) in methods {
                    if let Ok(v) = serde_json::to_value(op) {
                        path_obj.insert(method.clone(), v);
                    }
                }
                paths.insert(path.clone(), serde_json::Value::Object(path_obj));
            }
        }
        
        // Build schemas
        if let Some(schemas) = val.pointer_mut("/components/schemas").and_then(|v| v.as_object_mut()) {
            for (name, schema) in &self.schemas {
                schemas.insert(name.clone(), schema.to_json_value());
            }
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

#[derive(Debug, Clone)]
pub struct Schema {
    pub type_name: String,
    pub properties: HashMap<String, Property>,
    pub required: Vec<String>,
}

impl Schema {
    pub fn to_json_value(&self) -> serde_json::Value {
        let mut props = serde_json::Map::new();
        for (name, prop) in &self.properties {
            props.insert(name.clone(), prop.to_json_value());
        }
        let mut obj = serde_json::json!({
            "type": self.type_name,
            "properties": props,
        });
        if !self.required.is_empty() {
            obj["required"] = serde_json::to_value(&self.required).unwrap();
        }
        obj
    }
}

// Keep Serialize for backward compat but Schema.to_json_value is preferred
impl Serialize for Schema {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.to_json_value().serialize(serializer)
    }
}

#[derive(Debug, Clone)]
pub struct Property {
    pub type_name: String,
    pub format: Option<String>,
    pub min_length: Option<usize>,
    pub max_length: Option<usize>,
    pub ref_path: Option<String>,
    pub items: Option<Box<Property>>,
    pub nullable: bool,
}

impl Property {
    pub fn to_json_value(&self) -> serde_json::Value {
        // $ref property — nested struct
        if let Some(ref_path) = &self.ref_path {
            if self.nullable {
                return serde_json::json!({
                    "anyOf": [
                        { "$ref": ref_path },
                        { "type": "null" }
                    ]
                });
            }
            return serde_json::json!({ "$ref": ref_path });
        }

        let mut obj = serde_json::Map::new();

        if self.nullable {
            // nullable via anyOf
            let mut inner = serde_json::Map::new();
            inner.insert("type".into(), serde_json::Value::String(self.type_name.clone()));
            if let Some(f) = &self.format {
                inner.insert("format".into(), serde_json::Value::String(f.clone()));
            }
            if let Some(v) = self.min_length {
                inner.insert("minLength".into(), serde_json::Value::Number(v.into()));
            }
            if let Some(v) = self.max_length {
                inner.insert("maxLength".into(), serde_json::Value::Number(v.into()));
            }
            if let Some(items) = &self.items {
                inner.insert("items".into(), items.to_json_value());
            }
            obj.insert("anyOf".into(), serde_json::json!([
                serde_json::Value::Object(inner),
                { "type": "null" }
            ]));
        } else {
            obj.insert("type".into(), serde_json::Value::String(self.type_name.clone()));
            if let Some(f) = &self.format {
                obj.insert("format".into(), serde_json::Value::String(f.clone()));
            }
            if let Some(v) = self.min_length {
                obj.insert("minLength".into(), serde_json::Value::Number(v.into()));
            }
            if let Some(v) = self.max_length {
                obj.insert("maxLength".into(), serde_json::Value::Number(v.into()));
            }
            if let Some(items) = &self.items {
                obj.insert("items".into(), items.to_json_value());
            }
        }

        serde_json::Value::Object(obj)
    }
}

impl Serialize for Property {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.to_json_value().serialize(serializer)
    }
}

pub struct PropertyPatch {
    pub min_length: Option<usize>,
    pub max_length: Option<usize>,
    pub format: Option<String>,
}

/// Result of schema_from_schemars: the main schema + any nested definitions
pub struct SchemaResult {
    pub schema: Schema,
    pub nested: HashMap<String, Schema>,
}

/// Convert schemars schema to our OpenAPI schema
pub fn schema_from_schemars(_name: &str, root: &schemars::schema::RootSchema) -> Schema {
    let result = schema_from_schemars_full(_name, root);
    result.schema
}

/// Convert schemars schema to our OpenAPI schema, also returning nested definitions
pub fn schema_from_schemars_full(_name: &str, root: &schemars::schema::RootSchema) -> SchemaResult {
    let mut properties = HashMap::new();
    let mut required = Vec::new();

    if let Some(obj) = &root.schema.object {
        for (prop_name, prop_schema) in &obj.properties {
            let prop = property_from_schemars_schema(prop_schema, &root.definitions);
            properties.insert(prop_name.clone(), prop);
        }

        for req in &obj.required {
            required.push(req.clone());
        }
    }

    // Convert definitions to nested schemas
    let mut nested = HashMap::new();
    for (def_name, def_schema) in &root.definitions {
        if let schemars::schema::Schema::Object(obj) = def_schema {
            if let Some(obj_val) = &obj.object {
                let mut def_props = HashMap::new();
                let mut def_required = Vec::new();
                for (pname, pschema) in &obj_val.properties {
                    def_props.insert(pname.clone(), property_from_schemars_schema(pschema, &root.definitions));
                }
                for req in &obj_val.required {
                    def_required.push(req.clone());
                }
                nested.insert(def_name.clone(), Schema {
                    type_name: "object".to_string(),
                    properties: def_props,
                    required: def_required,
                });
            }
        }
    }

    SchemaResult {
        schema: Schema {
            type_name: "object".to_string(),
            properties,
            required,
        },
        nested,
    }
}

fn property_from_schemars_schema(
    schema: &schemars::schema::Schema,
    definitions: &schemars::Map<String, schemars::schema::Schema>,
) -> Property {
    match schema {
        schemars::schema::Schema::Object(obj) => {
            // Check for $ref (nested struct reference)
            if let Some(ref reference) = obj.reference {
                let ref_name = reference.trim_start_matches("#/definitions/");
                return Property {
                    type_name: "object".to_string(),
                    format: None,
                    min_length: None,
                    max_length: None,
                    ref_path: Some(format!("#/components/schemas/{}", ref_name)),
                    items: None,
                    nullable: false,
                };
            }

            // Check for anyOf (Option<T> in schemars)
            if let Some(subschemas) = &obj.subschemas {
                if let Some(any_of) = &subschemas.any_of {
                    // schemars Option<T> = anyOf: [{actual type}, {type: null}]
                    let non_null: Vec<_> = any_of.iter().filter(|s| {
                        if let schemars::schema::Schema::Object(o) = s {
                            if let Some(schemars::schema::SingleOrVec::Single(t)) = &o.instance_type {
                                return **t != schemars::schema::InstanceType::Null;
                            }
                            // Could be a $ref (no instance_type)
                            return o.reference.is_some() || o.object.is_some() || o.array.is_some();
                        }
                        false
                    }).collect();

                    if let Some(inner) = non_null.first() {
                        let mut prop = property_from_schemars_schema(inner, definitions);
                        prop.nullable = true;
                        return prop;
                    }
                }
            }

            // Check instance_type
            if let Some(ty) = &obj.instance_type {
                let type_name = match ty {
                    schemars::schema::SingleOrVec::Single(single) => format_instance_type(single),
                    schemars::schema::SingleOrVec::Vec(vec) => {
                        // Filter out null for nullable
                        let non_null: Vec<_> = vec.iter()
                            .filter(|t| **t != schemars::schema::InstanceType::Null)
                            .collect();
                        let has_null = vec.iter().any(|t| *t == schemars::schema::InstanceType::Null);
                        let tn = if let Some(first) = non_null.first() {
                            format_instance_type(first)
                        } else {
                            "string".to_string()
                        };
                        if has_null {
                            return Property {
                                type_name: tn,
                                format: None,
                                min_length: None,
                                max_length: None,
                                ref_path: None,
                                items: None,
                                nullable: true,
                            };
                        }
                        tn
                    }
                };

                // Handle array type (Vec<T>)
                if type_name == "array" {
                    let items_prop = if let Some(arr) = &obj.array {
                        if let Some(schemars::schema::SingleOrVec::Single(item_schema)) = &arr.items {
                            Some(Box::new(property_from_schemars_schema(item_schema, definitions)))
                        } else {
                            None
                        }
                    } else {
                        None
                    };

                    return Property {
                        type_name,
                        format: None,
                        min_length: None,
                        max_length: None,
                        ref_path: None,
                        items: items_prop,
                        nullable: false,
                    };
                }

                return Property {
                    type_name,
                    format: None,
                    min_length: None,
                    max_length: None,
                    ref_path: None,
                    items: None,
                    nullable: false,
                };
            }

            // Fallback
            Property {
                type_name: "string".to_string(),
                format: None,
                min_length: None,
                max_length: None,
                ref_path: None,
                items: None,
                nullable: false,
            }
        }
        _ => Property {
            type_name: "string".to_string(),
            format: None,
            min_length: None,
            max_length: None,
            ref_path: None,
            items: None,
            nullable: false,
        },
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
