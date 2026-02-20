use proc_macro::TokenStream;
use quote::{quote, format_ident};
use syn::{parse_macro_input, ItemFn, ItemStruct, ItemEnum, FnArg, PatType, Type, PathSegment, LitStr, LitInt};

fn extract_inner_type(seg: &PathSegment) -> Option<&Type> {
    if let syn::PathArguments::AngleBracketed(args) = &seg.arguments {
        if let Some(syn::GenericArgument::Type(inner)) = args.args.first() {
            return Some(inner);
        }
    }
    None
}

fn is_dep_type(ty: &Type) -> bool {
    if let Type::Path(tp) = ty {
        if let Some(seg) = tp.path.segments.last() {
            return seg.ident == "Dep";
        }
    }
    false
}

fn is_state_type(ty: &Type) -> bool {
    if let Type::Path(tp) = ty {
        if let Some(seg) = tp.path.segments.last() {
            return seg.ident == "State";
        }
    }
    false
}

fn is_auth_type(ty: &Type) -> bool {
    if let Type::Path(tp) = ty {
        if let Some(seg) = tp.path.segments.last() {
            return seg.ident == "Auth";
        }
    }
    false
}

fn is_query_type(ty: &Type) -> bool {
    if let Type::Path(tp) = ty {
        if let Some(seg) = tp.path.segments.last() {
            return seg.ident == "Query";
        }
    }
    false
}

fn get_type_name(ty: &Type) -> String {
    if let Type::Path(tp) = ty {
        if let Some(seg) = tp.path.segments.last() {
            return seg.ident.to_string();
        }
    }
    "Unknown".to_string()
}

/// Check if the type is Result<T, E> and return the Ok type
fn get_result_ok_type(ty: &Type) -> Option<&Type> {
    if let Type::Path(tp) = ty {
        if let Some(seg) = tp.path.segments.last() {
            if seg.ident == "Result" {
                if let syn::PathArguments::AngleBracketed(args) = &seg.arguments {
                    if let Some(syn::GenericArgument::Type(ok_type)) = args.args.first() {
                        return Some(ok_type);
                    }
                }
            }
        }
    }
    None
}

/// Check if the type is Vec<T> and return the inner type name
fn get_vec_inner_type_name(ty: &Type) -> Option<String> {
    if let Type::Path(tp) = ty {
        if let Some(seg) = tp.path.segments.last() {
            if seg.ident == "Vec" {
                if let Some(inner) = extract_inner_type(seg) {
                    return Some(get_type_name(inner));
                }
            }
        }
    }
    None
}

fn is_primitive_type(ty: &Type) -> bool {
    let name = get_type_name(ty);
    matches!(name.as_str(), "i8"|"i16"|"i32"|"i64"|"i128"|"u8"|"u16"|"u32"|"u64"|"u128"|"f32"|"f64"|"String"|"bool")
}

/// Extract doc comment string from attributes
fn extract_doc_comment(attrs: &[syn::Attribute]) -> String {
    let mut lines = Vec::new();
    for attr in attrs {
        if attr.path().is_ident("doc") {
            if let syn::Meta::NameValue(nv) = &attr.meta {
                if let syn::Expr::Lit(syn::ExprLit { lit: syn::Lit::Str(s), .. }) = &nv.value {
                    lines.push(s.value().trim().to_string());
                }
            }
        }
    }
    lines.join("\n").trim().to_string()
}

fn route_macro_impl(method: &str, attr: TokenStream, item: TokenStream) -> TokenStream {
    let path = parse_macro_input!(attr as LitStr).value();
    let input_fn = parse_macro_input!(item as ItemFn);
    let fn_name = &input_fn.sig.ident;
    let fn_vis = &input_fn.vis;
    let fn_sig = &input_fn.sig;
    let fn_block = &input_fn.block;

    // Parse custom attributes: #[status(N)], #[tag("x")], #[security("x")], doc comments
    let mut status_code: Option<u16> = None;
    let mut tags: Vec<String> = Vec::new();
    let mut security_schemes: Vec<String> = Vec::new();
    let description = extract_doc_comment(&input_fn.attrs);

    let mut clean_attrs: Vec<&syn::Attribute> = Vec::new();
    for attr in &input_fn.attrs {
        if attr.path().is_ident("status") {
            if let syn::Meta::List(list) = &attr.meta {
                let tokens = list.tokens.clone();
                if let Ok(lit) = syn::parse2::<LitInt>(tokens) {
                    status_code = Some(lit.base10_parse().unwrap());
                }
            }
        } else if attr.path().is_ident("tag") {
            if let syn::Meta::List(list) = &attr.meta {
                let tokens = list.tokens.clone();
                if let Ok(lit) = syn::parse2::<LitStr>(tokens) {
                    tags.push(lit.value());
                }
            }
        } else if attr.path().is_ident("security") {
            if let syn::Meta::List(list) = &attr.meta {
                let tokens = list.tokens.clone();
                if let Ok(lit) = syn::parse2::<LitStr>(tokens) {
                    security_schemes.push(lit.value());
                }
            }
        } else if !attr.path().is_ident("doc") {
            clean_attrs.push(attr);
        } else {
            clean_attrs.push(attr);
        }
    }

    // Default status codes
    let default_status: u16 = match method {
        "post" => 201,
        "delete" => 204,
        _ => 200,
    };
    let success_status = status_code.unwrap_or(default_status);
    let has_auth = input_fn.sig.inputs.iter().any(|arg| {
        if let FnArg::Typed(PatType { ty, .. }) = arg { is_auth_type(ty) } else { false }
    });

    let path_params: Vec<String> = path.split('/')
        .filter(|s| s.starts_with('{') && s.ends_with('}'))
        .map(|s| s[1..s.len()-1].to_string())
        .collect();

    let axum_path = path.clone();
    let method_upper = method.to_uppercase();
    let method_ident = format_ident!("{}", method.to_lowercase());
    let wrapper_name = format_ident!("__{}_axum_handler", fn_name);
    let route_info_name = format_ident!("__{}_route_info", fn_name);
    let route_ref_name = format_ident!("__HAYAI_ROUTE_{}", fn_name.to_string().to_uppercase());

    let mut dep_extractions = Vec::new();
    let mut call_args = Vec::new();
    let mut has_body = false;
    let mut body_type: Option<&Type> = None;
    let mut path_param_types: Vec<(&syn::Ident, &Type)> = Vec::new();
    let mut query_type: Option<&Type> = None;
    let mut query_extraction = quote!{};

    for arg in &input_fn.sig.inputs {
        if let FnArg::Typed(PatType { pat, ty, .. }) = arg {
            let param_name = quote!(#pat).to_string();
            if is_dep_type(ty) {
                if let Type::Path(tp) = ty.as_ref() {
                    if let Some(seg) = tp.path.segments.last() {
                        if let Some(inner) = extract_inner_type(seg) {
                            dep_extractions.push(quote! {
                                let #pat: hayai::Dep<#inner> = hayai::Dep::from_app_state(&state)?;
                            });
                            call_args.push(quote!(#pat));
                        }
                    }
                }
            } else if is_state_type(ty) {
                if let Type::Path(tp) = ty.as_ref() {
                    if let Some(seg) = tp.path.segments.last() {
                        if let Some(inner) = extract_inner_type(seg) {
                            dep_extractions.push(quote! {
                                let #pat: hayai::State<#inner> = hayai::State::from_app_state(&state)?;
                            });
                            call_args.push(quote!(#pat));
                        }
                    }
                }
            } else if is_auth_type(ty) {
                if let Type::Path(tp) = ty.as_ref() {
                    if let Some(seg) = tp.path.segments.last() {
                        if let Some(inner) = extract_inner_type(seg) {
                            dep_extractions.push(quote! {
                                let #pat: hayai::Auth<#inner> = hayai::Auth::from_request_parts(&parts).await?;
                            });
                            call_args.push(quote!(#pat));
                        }
                    }
                }
            } else if is_query_type(ty) {
                if let Type::Path(tp) = ty.as_ref() {
                    if let Some(seg) = tp.path.segments.last() {
                        if let Some(inner) = extract_inner_type(seg) {
                            query_type = Some(inner);
                            query_extraction = quote! {
                                let #pat: hayai::axum::extract::Query<#inner> =
                                    hayai::axum::extract::Query::from_request_parts(&mut parts, &state).await
                                    .map_err(|e| hayai::ApiError::bad_request(format!("Invalid query parameters: {}", e)))?;
                            };
                            call_args.push(quote!(#pat));
                        }
                    }
                }
            } else if path_params.contains(&param_name) {
                if let syn::Pat::Ident(pi) = pat.as_ref() {
                    path_param_types.push((&pi.ident, ty));
                    call_args.push(quote!(#pat));
                }
            } else if !is_primitive_type(ty) {
                has_body = true;
                body_type = Some(ty);
                call_args.push(quote!(#pat));
            } else {
                call_args.push(quote!(#pat));
            }
        }
    }

    let return_type = match &input_fn.sig.output {
        syn::ReturnType::Type(_, ty) => Some(ty.as_ref()),
        _ => None,
    };

    // Detect Result<T, ApiError> return type
    let is_result_return = return_type.map(|t| get_result_ok_type(t).is_some()).unwrap_or(false);
    let effective_return_type = return_type.and_then(|t| get_result_ok_type(t)).or(return_type);

    let return_type_name = effective_return_type.map(|t| get_type_name(t)).unwrap_or_else(|| "()".to_string());

    // Detect Vec<T> return type for array schema (check effective type, i.e. inside Result if applicable)
    let is_vec_response = effective_return_type.map(|t| get_vec_inner_type_name(t).is_some()).unwrap_or(false);
    let vec_inner_type_name = effective_return_type.and_then(|t| get_vec_inner_type_name(t)).unwrap_or_default();

    let path_extraction = if !path_param_types.is_empty() {
        let names: Vec<_> = path_param_types.iter().map(|(n,_)| *n).collect();
        let types: Vec<_> = path_param_types.iter().map(|(_,t)| *t).collect();
        if path_param_types.len() == 1 {
            let n = names[0]; let t = types[0];
            quote! {
                let hayai::axum::extract::Path(#n): hayai::axum::extract::Path<#t> =
                    hayai::axum::extract::Path::from_request_parts(&mut parts, &state).await
                    .map_err(|e| hayai::ApiError::bad_request(format!("Invalid path param: {}", e)))?;
            }
        } else {
            quote! {
                let hayai::axum::extract::Path((#(#names),*)): hayai::axum::extract::Path<(#(#types),*)> =
                    hayai::axum::extract::Path::from_request_parts(&mut parts, &state).await
                    .map_err(|e| hayai::ApiError::bad_request(format!("Invalid path params: {}", e)))?;
            }
        }
    } else {
        quote!{}
    };

    let body_extraction = if has_body {
        let bty = body_type.unwrap();
        let bpat = input_fn.sig.inputs.iter().find_map(|arg| {
            if let FnArg::Typed(PatType { pat, ty, .. }) = arg {
                if !is_dep_type(ty) && !is_state_type(ty) && !is_auth_type(ty) && !is_primitive_type(ty) && !is_query_type(ty) {
                    let n = quote!(#pat).to_string();
                    if !path_params.contains(&n) { return Some(pat.clone()); }
                }
            }
            None
        }).unwrap();
        quote! {
            let hayai::axum::Json(#bpat): hayai::axum::Json<#bty> =
                hayai::axum::Json::from_request(req, &state).await
                .map_err(|e| hayai::ApiError::bad_request(format!("Invalid body: {}", e)))?;
            #bpat.validate().map_err(|e| hayai::ApiError::validation_error(e))?;
        }
    } else {
        quote! { let _ = req; }
    };

    // Generate response based on status code
    let status_lit = proc_macro2::Literal::u16_unsuffixed(success_status);
    let response_expr = if success_status == 204 {
        if is_result_return {
            quote! {
                let _ = #fn_name(#(#call_args),*).await?;
                Ok((hayai::axum::http::StatusCode::from_u16(#status_lit).unwrap(),).into_response())
            }
        } else {
            quote! {
                let _ = #fn_name(#(#call_args),*).await;
                Ok((hayai::axum::http::StatusCode::from_u16(#status_lit).unwrap(),).into_response())
            }
        }
    } else if is_result_return {
        quote! {
            let result = #fn_name(#(#call_args),*).await?;
            let value = hayai::serde_json::to_value(&result)
                .map_err(|e| hayai::ApiError::internal(format!("Response serialization failed: {}", e)))?;
            Ok((
                hayai::axum::http::StatusCode::from_u16(#status_lit).unwrap(),
                hayai::axum::Json(value),
            ).into_response())
        }
    } else {
        quote! {
            let result = #fn_name(#(#call_args),*).await;
            let value = hayai::serde_json::to_value(&result)
                .map_err(|e| hayai::ApiError::internal(format!("Response serialization failed: {}", e)))?;
            Ok((
                hayai::axum::http::StatusCode::from_u16(#status_lit).unwrap(),
                hayai::axum::Json(value),
            ).into_response())
        }
    };

    let path_param_schemas: Vec<_> = path_params.iter().map(|p| {
        // Find the type of this path param
        let openapi_type = path_param_types.iter()
            .find(|(name, _)| name.to_string() == *p)
            .map(|(_, ty)| {
                let type_name = get_type_name(ty);
                match type_name.as_str() {
                    "i8"|"i16"|"i32"|"i64"|"i128"|"u8"|"u16"|"u32"|"u64"|"u128" => "integer",
                    "f32"|"f64" => "number",
                    "String" => "string",
                    "bool" => "boolean",
                    _ => "string",
                }
            })
            .unwrap_or("string");
        quote! {
            hayai::openapi::Parameter {
                name: #p,
                location: "path",
                required: true,
                schema: hayai::openapi::SchemaObject::new_type(#openapi_type),
                description: None,
            }
        }
    }).collect();

    let body_type_name = body_type.map(|t| get_type_name(t)).unwrap_or_default();
    let fn_name_str = fn_name.to_string();

    let query_params_fn_expr = if let Some(qt) = query_type {
        quote! { Some(|| {
            let root = hayai::schemars::schema_for!(#qt);
            hayai::openapi::query_params_from_schema(&root)
        }) }
    } else {
        quote! { None }
    };

    let output = quote! {
        #(#clean_attrs)*
        #fn_vis #fn_sig #fn_block

        #[doc(hidden)]
        async fn #wrapper_name(
            hayai::axum::extract::State(state): hayai::axum::extract::State<hayai::AppState>,
            mut parts: hayai::axum::http::request::Parts,
            req: hayai::axum::http::Request<hayai::axum::body::Body>,
        ) -> Result<hayai::axum::response::Response, hayai::ApiError> {
            use hayai::axum::extract::FromRequest;
            use hayai::axum::extract::FromRequestParts;
            use hayai::axum::response::IntoResponse;
            use hayai::Validate;

            #path_extraction
            #query_extraction
            #(#dep_extractions)*
            #body_extraction

            #response_expr
        }

        #[doc(hidden)]
        #[allow(non_upper_case_globals)]
        static #route_info_name: hayai::RouteInfo = hayai::RouteInfo {
            path: #path,
            axum_path: #axum_path,
            method: #method_upper,
            handler_name: #fn_name_str,
            response_type_name: #return_type_name,
            is_result_return: #is_result_return,
            is_vec_response: #is_vec_response,
            vec_inner_type_name: #vec_inner_type_name,
            parameters: &[#(#path_param_schemas),*],
            has_body: #has_body,
            body_type_name: #body_type_name,
            success_status: #status_lit,
            description: #description,
            tags: &[#(#tags),*],
            security: &[#(#security_schemes),*],
            has_auth: #has_auth,
            query_params_fn: #query_params_fn_expr,
            register_fn: |app: hayai::axum::Router<hayai::AppState>| {
                app.route(#axum_path, hayai::axum::routing::#method_ident(#wrapper_name))
            },
            method_router_fn: || {
                hayai::axum::routing::#method_ident(#wrapper_name)
            },
        };

        #[doc(hidden)]
        pub static #route_ref_name: &hayai::RouteInfo = &#route_info_name;

        hayai::inventory::submit! { &#route_info_name }
    };

    output.into()
}

#[proc_macro_attribute]
pub fn get(attr: TokenStream, item: TokenStream) -> TokenStream {
    route_macro_impl("get", attr, item)
}

#[proc_macro_attribute]
pub fn post(attr: TokenStream, item: TokenStream) -> TokenStream {
    route_macro_impl("post", attr, item)
}

#[proc_macro_attribute]
pub fn put(attr: TokenStream, item: TokenStream) -> TokenStream {
    route_macro_impl("put", attr, item)
}

#[proc_macro_attribute]
pub fn delete(attr: TokenStream, item: TokenStream) -> TokenStream {
    route_macro_impl("delete", attr, item)
}

#[proc_macro_attribute]
pub fn api_model(attr: TokenStream, item: TokenStream) -> TokenStream {
    // Parse the attribute to look for validate(custom = "fn_name")
    let mut custom_validation_fn: Option<proc_macro2::TokenStream> = None;
    
    // Parse attribute if provided (e.g., #[api_model(validate(custom = "fn_name"))])
    let attr_str = attr.to_string();
    if attr_str.contains("validate") {
        // Simple string parsing to find validate(custom = "fn_name")
        if let Some(start) = attr_str.find("custom") {
            let rest = &attr_str[start..];
            if let Some(eq_pos) = rest.find('=') {
                let after_eq = &rest[eq_pos+1..];
                // Find the quoted string
                if let Some(quote_start) = after_eq.find('"') {
                    if let Some(quote_end) = after_eq[quote_start+1..].find('"') {
                        let fn_name = &after_eq[quote_start+1..quote_start+1+quote_end];
                        if !fn_name.is_empty() {
                            custom_validation_fn = Some(quote!(#fn_name));
                        }
                    }
                }
            }
        }
    }
    
    let item_clone = item.clone();
    if let Ok(input) = syn::parse::<ItemStruct>(item) {
        api_model_struct(input, custom_validation_fn)
    } else if let Ok(input) = syn::parse::<ItemEnum>(item_clone) {
        api_model_enum(input)
    } else {
        syn::Error::new(proc_macro2::Span::call_site(), "api_model only supports structs and enums")
            .to_compile_error()
            .into()
    }
}

fn api_model_enum(input: ItemEnum) -> TokenStream {
    let name = &input.ident;
    let vis = &input.vis;
    let attrs = &input.attrs;
    let variants = &input.variants;
    let description = extract_doc_comment(attrs);

    let variant_names: Vec<String> = variants.iter()
        .map(|v| v.ident.to_string())
        .collect();

    let name_str = name.to_string();

    let desc_expr = if description.is_empty() {
        quote! { None }
    } else {
        quote! { Some(#description.to_string()) }
    };

    let output = quote! {
        #(#attrs)*
        #[derive(hayai::serde::Serialize, hayai::serde::Deserialize, hayai::schemars::JsonSchema)]
        #[serde(crate = "hayai::serde")]
        #[schemars(crate = "hayai::schemars")]
        #vis enum #name {
            #variants
        }

        impl hayai::Validate for #name {
            fn validate(&self) -> Result<(), Vec<String>> { Ok(()) }
        }

        hayai::inventory::submit! {
            hayai::SchemaInfo {
                name: #name_str,
                schema_fn: || {
                    static CACHE: std::sync::OnceLock<hayai::openapi::Schema> = std::sync::OnceLock::new();
                    CACHE.get_or_init(|| {
                        hayai::openapi::Schema {
                            type_name: "string".to_string(),
                            properties: std::collections::HashMap::new(),
                            required: vec![],
                            description: #desc_expr,
                            enum_values: Some(vec![#(#variant_names.to_string()),*]),
                            example: None,
                        }
                    }).clone()
                },
                nested_fn: || {
                    static CACHE: std::sync::OnceLock<std::collections::HashMap<String, hayai::openapi::Schema>> = std::sync::OnceLock::new();
                    CACHE.get_or_init(|| std::collections::HashMap::new()).clone()
                },
            }
        }
    };

    output.into()
}

fn api_model_struct(input: ItemStruct, custom_validation_fn: Option<proc_macro2::TokenStream>) -> TokenStream {
    let name = &input.ident;
    let vis = &input.vis;
    let attrs = &input.attrs;
    let generics = &input.generics;
    let struct_description = extract_doc_comment(attrs);

    let fields = match &input.fields {
        syn::Fields::Named(fields) => &fields.named,
        _ => return syn::Error::new_spanned(&input, "api_model only supports structs with named fields")
            .to_compile_error()
            .into(),
    };

    let mut validation_checks = Vec::new();
    let mut schema_patches = Vec::new();
    let mut clean_fields = Vec::new();

    for field in fields {
        let field_name = field.ident.as_ref().unwrap();
        let field_name_str = field_name.to_string();

        // Extract doc comment for field description
        let field_desc = extract_doc_comment(&field.attrs);
        if !field_desc.is_empty() {
            schema_patches.push(quote! {
                if let Some(prop) = props.get_mut(#field_name_str) {
                    prop.description = Some(#field_desc.to_string());
                }
            });
        }

        for attr in &field.attrs {
            if attr.path().is_ident("validate") {
                let _ = attr.parse_nested_meta(|meta| {
                    if meta.path.is_ident("min_length") {
                        let value = meta.value()?;
                        let lit: syn::LitInt = value.parse()?;
                        let min: usize = lit.base10_parse()?;
                        validation_checks.push(quote! {
                            if self.#field_name.len() < #min {
                                errors.push(format!("{}: must be at least {} characters", #field_name_str, #min));
                            }
                        });
                        schema_patches.push(quote! {
                            if let Some(prop) = props.get_mut(#field_name_str) {
                                prop.min_length = Some(#min);
                            }
                        });
                    } else if meta.path.is_ident("max_length") {
                        let value = meta.value()?;
                        let lit: syn::LitInt = value.parse()?;
                        let max: usize = lit.base10_parse()?;
                        validation_checks.push(quote! {
                            if self.#field_name.len() > #max {
                                errors.push(format!("{}: must be at most {} characters", #field_name_str, #max));
                            }
                        });
                        schema_patches.push(quote! {
                            if let Some(prop) = props.get_mut(#field_name_str) {
                                prop.max_length = Some(#max);
                            }
                        });
                    } else if meta.path.is_ident("email") {
                        validation_checks.push(quote! {
                            {
                                let email = &self.#field_name;
                                let at_count = email.chars().filter(|&c| c == '@').count();
                                let valid = at_count == 1
                                    && !email.starts_with('@')
                                    && !email.ends_with('@')
                                    && {
                                        if let Some(at_pos) = email.find('@') {
                                            let domain = &email[at_pos + 1..];
                                            !domain.is_empty() && domain.contains('.')
                                                && !domain.starts_with('.') && !domain.ends_with('.')
                                        } else {
                                            false
                                        }
                                    };
                                if !valid {
                                    errors.push(format!("{}: must be a valid email address", #field_name_str));
                                }
                            }
                        });
                        schema_patches.push(quote! {
                            if let Some(prop) = props.get_mut(#field_name_str) {
                                prop.format = Some("email".to_string());
                            }
                        });
                    } else if meta.path.is_ident("minimum") {
                        let value = meta.value()?;
                        let lit: syn::LitInt = value.parse()?;
                        let min: i64 = lit.base10_parse()?;
                        let min_f64 = min as f64;
                        validation_checks.push(quote! {
                            if (self.#field_name as f64) < #min_f64 {
                                errors.push(format!("{}: must be at least {}", #field_name_str, #min));
                            }
                        });
                        schema_patches.push(quote! {
                            if let Some(prop) = props.get_mut(#field_name_str) {
                                prop.minimum = Some(#min_f64);
                            }
                        });
                    } else if meta.path.is_ident("maximum") {
                        let value = meta.value()?;
                        let lit: syn::LitInt = value.parse()?;
                        let max: i64 = lit.base10_parse()?;
                        let max_f64 = max as f64;
                        validation_checks.push(quote! {
                            if (self.#field_name as f64) > #max_f64 {
                                errors.push(format!("{}: must be at most {}", #field_name_str, #max));
                            }
                        });
                        schema_patches.push(quote! {
                            if let Some(prop) = props.get_mut(#field_name_str) {
                                prop.maximum = Some(#max_f64);
                            }
                        });
                    } else if meta.path.is_ident("pattern") {
                        let value = meta.value()?;
                        let lit: syn::LitStr = value.parse()?;
                        let pat = lit.value();
                        validation_checks.push(quote! {
                            {
                                static RE: std::sync::OnceLock<hayai::regex::Regex> = std::sync::OnceLock::new();
                                let re = RE.get_or_init(|| hayai::regex::Regex::new(#pat).expect("Invalid regex"));
                                if !re.is_match(&self.#field_name) {
                                    errors.push(format!("{}: must match pattern {}", #field_name_str, #pat));
                                }
                            }
                        });
                        schema_patches.push(quote! {
                            if let Some(prop) = props.get_mut(#field_name_str) {
                                prop.pattern = Some(#pat.to_string());
                            }
                        });
                    } else if meta.path.is_ident("min_items") {
                        let value = meta.value()?;
                        let lit: syn::LitInt = value.parse()?;
                        let min: usize = lit.base10_parse()?;
                        validation_checks.push(quote! {
                            if self.#field_name.len() < #min {
                                errors.push(format!("{}: must have at least {} items", #field_name_str, #min));
                            }
                        });
                        schema_patches.push(quote! {
                            if let Some(prop) = props.get_mut(#field_name_str) {
                                prop.min_items = Some(#min);
                            }
                        });
                    }
                    Ok(())
                });
            } else if attr.path().is_ident("schema") {
                let _ = attr.parse_nested_meta(|meta| {
                    if meta.path.is_ident("example") {
                        let value = meta.value()?;
                        let lit: syn::LitStr = value.parse()?;
                        let example_val = lit.value();
                        schema_patches.push(quote! {
                            if let Some(prop) = props.get_mut(#field_name_str) {
                                prop.example = Some(#example_val.to_string());
                            }
                        });
                    }
                    Ok(())
                });
            }
        }

        let mut clean_field = field.clone();
        clean_field.attrs.retain(|a| !a.path().is_ident("validate") && !a.path().is_ident("schema"));
        clean_fields.push(clean_field);
    }

    let name_str = name.to_string();

    let desc_expr = if struct_description.is_empty() {
        quote! { None }
    } else {
        quote! { Some(#struct_description.to_string()) }
    };

    let output = quote! {
        #(#attrs)*
        #[derive(hayai::serde::Serialize, hayai::serde::Deserialize, hayai::schemars::JsonSchema)]
        #[serde(crate = "hayai::serde")]
        #[schemars(crate = "hayai::schemars")]
        #vis struct #name #generics {
            #(#clean_fields),*
        }

        impl hayai::Validate for #name {
            fn validate(&self) -> Result<(), Vec<String>> {
                let mut errors = Vec::new();
                #(#validation_checks)*
                if errors.is_empty() { Ok(()) } else { Err(errors) }
            }
        }

        impl hayai::HasSchemaPatches for #name {
            fn patch_schema(props: &mut std::collections::HashMap<String, hayai::openapi::PropertyPatch>) {
                #(#schema_patches)*
            }
        }

        hayai::inventory::submit! {
            hayai::SchemaInfo {
                name: #name_str,
                schema_fn: || {
                    static CACHE: std::sync::OnceLock<hayai::openapi::Schema> = std::sync::OnceLock::new();
                    CACHE.get_or_init(|| {
                        let base = hayai::schemars::schema_for!(#name);
                        let result = hayai::openapi::schema_from_schemars_full(#name_str, &base);
                        let mut schema = result.schema;
                        schema.description = #desc_expr;
                        let mut patches = std::collections::HashMap::new();
                        for (name, _) in &schema.properties {
                            patches.insert(name.clone(), hayai::openapi::PropertyPatch::default());
                        }
                        <#name as hayai::HasSchemaPatches>::patch_schema(&mut patches);
                        for (name, patch) in patches {
                            if let Some(prop) = schema.properties.get_mut(&name) {
                                if patch.min_length.is_some() { prop.min_length = patch.min_length; }
                                if patch.max_length.is_some() { prop.max_length = patch.max_length; }
                                if patch.format.is_some() { prop.format = patch.format; }
                                if patch.minimum.is_some() { prop.minimum = patch.minimum; }
                                if patch.maximum.is_some() { prop.maximum = patch.maximum; }
                                if patch.pattern.is_some() { prop.pattern = patch.pattern.clone(); }
                                if patch.min_items.is_some() { prop.min_items = patch.min_items; }
                                if patch.description.is_some() { prop.description = patch.description.clone(); }
                                if patch.example.is_some() { prop.example = patch.example.clone(); }
                            }
                        }
                        schema
                    }).clone()
                },
                nested_fn: || {
                    static CACHE: std::sync::OnceLock<std::collections::HashMap<String, hayai::openapi::Schema>> = std::sync::OnceLock::new();
                    CACHE.get_or_init(|| {
                        let base = hayai::schemars::schema_for!(#name);
                        let result = hayai::openapi::schema_from_schemars_full(#name_str, &base);
                        result.nested
                    }).clone()
                },
            }
        }
    };

    output.into()
}
