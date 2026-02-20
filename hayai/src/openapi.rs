use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize)]
pub struct OpenApiSpec {
    pub openapi: String,
    pub info: Info,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub servers: Vec<Server>,
    pub paths: HashMap<String, HashMap<String, Operation>>,
    #[serde(rename = "components")]
    pub schemas: HashMap<String, Schema>,
    #[serde(skip)]
    pub security_schemes: HashMap<String, SecurityScheme>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Server {
    pub url: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SecurityScheme {
    #[serde(rename = "type")]
    pub scheme_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scheme: Option<String>,
    #[serde(rename = "bearerFormat", skip_serializing_if = "Option::is_none")]
    pub bearer_format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(rename = "in", skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
}

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

        if !self.servers.is_empty() {
            val["servers"] = serde_json::to_value(&self.servers).unwrap();
        }

        if !self.security_schemes.is_empty() {
            let mut schemes = serde_json::Map::new();
            for (name, scheme) in &self.security_schemes {
                schemes.insert(name.clone(), serde_json::to_value(scheme).unwrap());
            }
            val["components"]["securitySchemes"] = serde_json::Value::Object(schemes);
        }

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
    pub description: Option<String>,
    pub operation_id: Option<String>,
    pub tags: Vec<String>,
    pub parameters: Vec<Parameter>,
    pub request_body: Option<RequestBody>,
    pub responses: HashMap<String, ResponseDef>,
    pub security: Vec<HashMap<String, Vec<String>>>,
}

impl Serialize for Operation {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(None)?;
        if !self.tags.is_empty() { map.serialize_entry("tags", &self.tags)?; }
        if let Some(s) = &self.summary { map.serialize_entry("summary", s)?; }
        if let Some(s) = &self.description { map.serialize_entry("description", s)?; }
        if let Some(s) = &self.operation_id { map.serialize_entry("operationId", s)?; }
        if !self.parameters.is_empty() { map.serialize_entry("parameters", &self.parameters)?; }
        if let Some(rb) = &self.request_body {
            map.serialize_entry("requestBody", &rb.to_json_value())?;
        }
        if !self.security.is_empty() {
            map.serialize_entry("security", &self.security)?;
        }
        let mut resp = serde_json::Map::new();
        for (code, r) in &self.responses {
            let mut obj = serde_json::Map::new();
            obj.insert("description".into(), serde_json::Value::String(r.description.clone()));
            if let Some(schema_ref) = &r.schema_ref {
                let content = serde_json::json!({
                    "application/json": {
                        "schema": schema_ref
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<&'static str>,
}

/// Dynamic parameter (owned strings, for query params generated at runtime)
#[derive(Debug, Clone)]
pub struct DynParameter {
    pub name: String,
    pub location: String,
    pub required: bool,
    pub schema_type: String,
    pub description: Option<String>,
}

impl Serialize for DynParameter {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("name", &self.name)?;
        map.serialize_entry("in", &self.location)?;
        map.serialize_entry("required", &self.required)?;
        map.serialize_entry("schema", &serde_json::json!({"type": self.schema_type}))?;
        if let Some(desc) = &self.description {
            map.serialize_entry("description", desc)?;
        }
        map.end()
    }
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
    pub schema_ref: Option<serde_json::Value>,
}

#[derive(Debug, Clone)]
pub struct Schema {
    pub type_name: String,
    pub properties: HashMap<String, Property>,
    pub required: Vec<String>,
    pub description: Option<String>,
    pub enum_values: Option<Vec<String>>,
    pub example: Option<String>,
}

impl Schema {
    pub fn to_json_value(&self) -> serde_json::Value {
        // Enum schema
        if let Some(values) = &self.enum_values {
            let mut obj = serde_json::json!({
                "type": self.type_name,
                "enum": values,
            });
            if let Some(desc) = &self.description {
                obj["description"] = serde_json::Value::String(desc.clone());
            }
            return obj;
        }

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
        if let Some(desc) = &self.description {
            obj["description"] = serde_json::Value::String(desc.clone());
        }
        obj
    }
}

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
    pub minimum: Option<f64>,
    pub maximum: Option<f64>,
    pub pattern: Option<String>,
    pub min_items: Option<usize>,
    pub description: Option<String>,
    pub ref_path: Option<String>,
    pub items: Option<Box<Property>>,
    pub nullable: bool,
    pub example: Option<String>,
}

impl Property {
    pub fn to_json_value(&self) -> serde_json::Value {
        if let Some(ref_path) = &self.ref_path {
            if self.nullable {
                return serde_json::json!({
                    "anyOf": [
                        { "$ref": ref_path },
                        { "type": "null" }
                    ]
                });
            }
            if let Some(desc) = &self.description {
                return serde_json::json!({
                    "allOf": [{ "$ref": ref_path }],
                    "description": desc,
                });
            }
            return serde_json::json!({ "$ref": ref_path });
        }

        let mut obj = serde_json::Map::new();

        if self.nullable {
            let mut inner = serde_json::Map::new();
            inner.insert("type".into(), serde_json::Value::String(self.type_name.clone()));
            self.add_constraints(&mut inner);
            obj.insert("anyOf".into(), serde_json::json!([
                serde_json::Value::Object(inner),
                { "type": "null" }
            ]));
        } else {
            obj.insert("type".into(), serde_json::Value::String(self.type_name.clone()));
            self.add_constraints(&mut obj);
        }

        if let Some(desc) = &self.description {
            obj.insert("description".into(), serde_json::Value::String(desc.clone()));
        }

        if let Some(example) = &self.example {
            obj.insert("example".into(), serde_json::Value::String(example.clone()));
        }

        serde_json::Value::Object(obj)
    }

    fn add_constraints(&self, obj: &mut serde_json::Map<String, serde_json::Value>) {
        if let Some(f) = &self.format {
            obj.insert("format".into(), serde_json::Value::String(f.clone()));
        }
        if let Some(v) = self.min_length {
            obj.insert("minLength".into(), serde_json::Value::Number(v.into()));
        }
        if let Some(v) = self.max_length {
            obj.insert("maxLength".into(), serde_json::Value::Number(v.into()));
        }
        if let Some(v) = self.minimum {
            obj.insert("minimum".into(), serde_json::json!(v));
        }
        if let Some(v) = self.maximum {
            obj.insert("maximum".into(), serde_json::json!(v));
        }
        if let Some(v) = &self.pattern {
            obj.insert("pattern".into(), serde_json::Value::String(v.clone()));
        }
        if let Some(v) = self.min_items {
            obj.insert("minItems".into(), serde_json::Value::Number(v.into()));
        }
        if let Some(items) = &self.items {
            obj.insert("items".into(), items.to_json_value());
        }
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
    pub minimum: Option<f64>,
    pub maximum: Option<f64>,
    pub pattern: Option<String>,
    pub min_items: Option<usize>,
    pub description: Option<String>,
    pub example: Option<String>,
}

/// Result of schema_from_schemars: the main schema + any nested definitions
pub struct SchemaResult {
    pub schema: Schema,
    pub nested: HashMap<String, Schema>,
}

pub fn schema_from_schemars(_name: &str, root: &schemars::schema::RootSchema) -> Schema {
    schema_from_schemars_full(_name, root).schema
}

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
                    description: None,
                    enum_values: None,
                    example: None,
                });
            }
        }
    }

    SchemaResult {
        schema: Schema {
            type_name: "object".to_string(),
            properties,
            required,
            description: None,
            enum_values: None,
            example: None,
        },
        nested,
    }
}

/// Extract query parameters from a schemars RootSchema
pub fn query_params_from_schema(root: &schemars::schema::RootSchema) -> Vec<DynParameter> {
    let mut params = Vec::new();
    if let Some(obj) = &root.schema.object {
        let required_set: std::collections::HashSet<&String> = obj.required.iter().collect();
        for (name, prop_schema) in &obj.properties {
            let type_name = schema_type_string(prop_schema);
            let description = schema_description(prop_schema);
            params.push(DynParameter {
                name: name.clone(),
                location: "query".to_string(),
                required: required_set.contains(name),
                schema_type: type_name,
                description,
            });
        }
    }
    params
}

/// Extract description from a schemars schema
fn schema_description(schema: &schemars::schema::Schema) -> Option<String> {
    match schema {
        schemars::schema::Schema::Object(obj) => {
            if let Some(meta) = &obj.metadata {
                return meta.description.clone();
            }
            // Check anyOf (Option<T>)
            if let Some(sub) = &obj.subschemas {
                if let Some(any_of) = &sub.any_of {
                    for s in any_of {
                        if let schemars::schema::Schema::Object(o) = s {
                            if let Some(meta) = &o.metadata {
                                if meta.description.is_some() {
                                    return meta.description.clone();
                                }
                            }
                        }
                    }
                }
            }
            None
        }
        _ => None,
    }
}

fn schema_type_string(schema: &schemars::schema::Schema) -> String {
    match schema {
        schemars::schema::Schema::Object(obj) => {
            if let Some(sub) = &obj.subschemas {
                if let Some(any_of) = &sub.any_of {
                    for s in any_of {
                        if let schemars::schema::Schema::Object(o) = s {
                            if let Some(schemars::schema::SingleOrVec::Single(t)) = &o.instance_type {
                                if **t != schemars::schema::InstanceType::Null {
                                    return format_instance_type(t);
                                }
                            }
                        }
                    }
                }
            }
            if let Some(ty) = &obj.instance_type {
                match ty {
                    schemars::schema::SingleOrVec::Single(t) => format_instance_type(t),
                    schemars::schema::SingleOrVec::Vec(vec) => {
                        vec.iter()
                            .find(|t| **t != schemars::schema::InstanceType::Null)
                            .map(|t| format_instance_type(t))
                            .unwrap_or_else(|| "string".to_string())
                    }
                }
            } else {
                "string".to_string()
            }
        }
        _ => "string".to_string(),
    }
}

/// Generate the standard ApiError schema
pub fn api_error_schema() -> Schema {
    let mut properties = HashMap::new();
    properties.insert("error".to_string(), Property {
        type_name: "string".to_string(),
        format: None, min_length: None, max_length: None,
        minimum: None, maximum: None, pattern: None, min_items: None,
        description: Some("Error message".to_string()),
        ref_path: None, items: None, nullable: false, example: None,
    });
    properties.insert("details".to_string(), Property {
        type_name: "array".to_string(),
        format: None, min_length: None, max_length: None,
        minimum: None, maximum: None, pattern: None, min_items: None,
        description: Some("Validation error details".to_string()),
        ref_path: None,
        items: Some(Box::new(Property {
            type_name: "string".to_string(),
            format: None, min_length: None, max_length: None,
            minimum: None, maximum: None, pattern: None, min_items: None,
            description: None, ref_path: None, items: None, nullable: false, example: None,
        })),
        nullable: false, example: None,
    });
    Schema {
        type_name: "object".to_string(),
        properties,
        required: vec!["error".to_string()],
        description: Some("Standard API error response".to_string()),
        enum_values: None,
        example: None,
    }
}

fn property_from_schemars_schema(
    schema: &schemars::schema::Schema,
    definitions: &schemars::Map<String, schemars::schema::Schema>,
) -> Property {
    match schema {
        schemars::schema::Schema::Object(obj) => {
            if let Some(ref reference) = obj.reference {
                let ref_name = reference.trim_start_matches("#/definitions/");
                return Property {
                    type_name: "object".to_string(),
                    format: None, min_length: None, max_length: None,
                    minimum: None, maximum: None, pattern: None, min_items: None,
                    description: None,
                    ref_path: Some(format!("#/components/schemas/{}", ref_name)),
                    items: None, nullable: false, example: None,
                };
            }

            if let Some(subschemas) = &obj.subschemas {
                if let Some(any_of) = &subschemas.any_of {
                    let non_null: Vec<_> = any_of.iter().filter(|s| {
                        if let schemars::schema::Schema::Object(o) = s {
                            if let Some(schemars::schema::SingleOrVec::Single(t)) = &o.instance_type {
                                return **t != schemars::schema::InstanceType::Null;
                            }
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

            if let Some(ty) = &obj.instance_type {
                let type_name = match ty {
                    schemars::schema::SingleOrVec::Single(single) => format_instance_type(single),
                    schemars::schema::SingleOrVec::Vec(vec) => {
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
                                format: None, min_length: None, max_length: None,
                                minimum: None, maximum: None, pattern: None, min_items: None,
                                description: None, ref_path: None, items: None, nullable: true, example: None,
                            };
                        }
                        tn
                    }
                };

                // Check if this is a string type that might actually be an enum
                // by looking at the enum_values in schemars metadata
                if type_name == "string" {
                    if let Some(enum_values) = &obj.enum_values {
                        // This is an inline enum - check if it matches a registered schema
                        // Try to find the type name from definitions
                        // For now, check all registered schemas for matching enum values
                        let enum_strs: Vec<String> = enum_values.iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect();
                        for info in inventory::iter::<crate::SchemaInfo> {
                            let schema = (info.schema_fn)();
                            if let Some(ref schema_enums) = schema.enum_values {
                                if *schema_enums == enum_strs {
                                    return Property {
                                        type_name: "string".to_string(),
                                        format: None, min_length: None, max_length: None,
                                        minimum: None, maximum: None, pattern: None, min_items: None,
                                        description: None,
                                        ref_path: Some(format!("#/components/schemas/{}", info.name)),
                                        items: None, nullable: false, example: None,
                                    };
                                }
                            }
                        }
                    }
                }

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
                        type_name, format: None, min_length: None, max_length: None,
                        minimum: None, maximum: None, pattern: None, min_items: None,
                        description: None, ref_path: None, items: items_prop, nullable: false, example: None,
                    };
                }

                return Property {
                    type_name, format: None, min_length: None, max_length: None,
                    minimum: None, maximum: None, pattern: None, min_items: None,
                    description: None, ref_path: None, items: None, nullable: false, example: None,
                };
            }

            Property {
                type_name: "string".to_string(),
                format: None, min_length: None, max_length: None,
                minimum: None, maximum: None, pattern: None, min_items: None,
                description: None, ref_path: None, items: None, nullable: false, example: None,
            }
        }
        _ => Property {
            type_name: "string".to_string(),
            format: None, min_length: None, max_length: None,
            minimum: None, maximum: None, pattern: None, min_items: None,
            description: None, ref_path: None, items: None, nullable: false, example: None,
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

/// Map status code to human-readable description
pub fn status_description(code: u16) -> &'static str {
    match code {
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        422 => "Validation Failed",
        500 => "Internal Server Error",
        _ => "Response",
    }
}
