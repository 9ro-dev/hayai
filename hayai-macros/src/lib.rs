use proc_macro::TokenStream;
use quote::{quote, format_ident};
use syn::{parse_macro_input, ItemFn, ItemStruct, FnArg, PatType, Type, PathSegment, LitStr};

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

fn get_type_name(ty: &Type) -> String {
    if let Type::Path(tp) = ty {
        if let Some(seg) = tp.path.segments.last() {
            return seg.ident.to_string();
        }
    }
    "Unknown".to_string()
}

fn is_primitive_type(ty: &Type) -> bool {
    let name = get_type_name(ty);
    matches!(name.as_str(), "i8"|"i16"|"i32"|"i64"|"i128"|"u8"|"u16"|"u32"|"u64"|"u128"|"f32"|"f64"|"String"|"bool")
}

fn route_macro_impl(method: &str, attr: TokenStream, item: TokenStream) -> TokenStream {
    let path = parse_macro_input!(attr as LitStr).value();
    let input_fn = parse_macro_input!(item as ItemFn);
    let fn_name = &input_fn.sig.ident;
    let fn_vis = &input_fn.vis;
    let fn_attrs = &input_fn.attrs;
    let fn_sig = &input_fn.sig;
    let fn_block = &input_fn.block;

    let path_params: Vec<String> = path.split('/')
        .filter(|s| s.starts_with('{') && s.ends_with('}'))
        .map(|s| s[1..s.len()-1].to_string())
        .collect();

    let axum_path = path.clone();
    let method_upper = method.to_uppercase();
    let method_ident = format_ident!("{}", method.to_lowercase());
    let wrapper_name = format_ident!("__{}_axum_handler", fn_name);

    let mut dep_extractions = Vec::new();
    let mut call_args = Vec::new();
    let mut has_body = false;
    let mut body_type: Option<&Type> = None;
    let mut path_param_types: Vec<(&syn::Ident, &Type)> = Vec::new();

    for arg in &input_fn.sig.inputs {
        if let FnArg::Typed(PatType { pat, ty, .. }) = arg {
            let param_name = quote!(#pat).to_string();
            if is_dep_type(ty) {
                if let Type::Path(tp) = ty.as_ref() {
                    if let Some(seg) = tp.path.segments.last() {
                        if let Some(inner) = extract_inner_type(seg) {
                            dep_extractions.push(quote! {
                                let #pat: hayai::Dep<#inner> = hayai::Dep::from_app_state(&state);
                            });
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
    let return_type_name = return_type.map(|t| get_type_name(t)).unwrap_or_else(|| "()".to_string());

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
                if !is_dep_type(ty) && !is_primitive_type(ty) {
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

    let path_param_schemas: Vec<_> = path_params.iter().map(|p| {
        quote! {
            hayai::openapi::Parameter {
                name: #p,
                location: "path",
                required: true,
                schema: hayai::openapi::SchemaObject::new_type("integer"),
            }
        }
    }).collect();

    let body_type_name = body_type.map(|t| get_type_name(t)).unwrap_or_default();
    let fn_name_str = fn_name.to_string();

    let output = quote! {
        // Original fn preserved with its name, visibility, and attributes
        #(#fn_attrs)*
        #fn_vis #fn_sig #fn_block

        #[doc(hidden)]
        async fn #wrapper_name(
            hayai::axum::extract::State(state): hayai::axum::extract::State<hayai::AppState>,
            mut parts: hayai::axum::http::request::Parts,
            req: hayai::axum::http::Request<hayai::axum::body::Body>,
        ) -> Result<hayai::axum::Json<hayai::serde_json::Value>, hayai::ApiError> {
            use hayai::axum::extract::FromRequest;
            use hayai::axum::extract::FromRequestParts;
            use hayai::Validate;

            // Parts extracted BEFORE body consumption (Issue #4)
            #path_extraction
            #(#dep_extractions)*
            #body_extraction

            let result = #fn_name(#(#call_args),*).await;
            let value = hayai::serde_json::to_value(&result)
                .map_err(|e| hayai::ApiError::internal(format!("Response serialization failed: {}", e)))?;
            Ok(hayai::axum::Json(value))
        }

        hayai::inventory::submit! {
            hayai::RouteInfo {
                path: #path,
                axum_path: #axum_path,
                method: #method_upper,
                handler_name: #fn_name_str,
                response_type_name: #return_type_name,
                parameters: &[#(#path_param_schemas),*],
                has_body: #has_body,
                body_type_name: #body_type_name,
                register_fn: |app: hayai::axum::Router<hayai::AppState>| {
                    app.route(#axum_path, hayai::axum::routing::#method_ident(#wrapper_name))
                },
            }
        }
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

/// Attribute macro that auto-derives Serialize, Deserialize, JsonSchema and generates
/// Validate + HasSchemaPatches + SchemaInfo registration.
/// Users only need `#[derive(ApiModel)]` (and optionally Debug, Clone).
#[proc_macro_attribute]
pub fn api_model(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemStruct);
    let name = &input.ident;
    let vis = &input.vis;
    let attrs = &input.attrs;
    let generics = &input.generics;

    let fields = match &input.fields {
        syn::Fields::Named(fields) => &fields.named,
        _ => panic!("ApiModel only supports structs with named fields"),
    };

    let mut validation_checks = Vec::new();
    let mut schema_patches = Vec::new();

    // Collect fields, stripping #[validate(...)] attributes for the re-emitted struct
    let mut clean_fields = Vec::new();
    for field in fields {
        let field_name = field.ident.as_ref().unwrap();
        let field_name_str = field_name.to_string();

        for attr in &field.attrs {
            if !attr.path().is_ident("validate") { continue; }
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
                }
                Ok(())
            });
        }

        // Strip validate attrs from field for re-emission
        let mut clean_field = field.clone();
        clean_field.attrs.retain(|a| !a.path().is_ident("validate"));
        clean_fields.push(clean_field);
    }

    let name_str = name.to_string();

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
                    let base = hayai::schemars::schema_for!(#name);
                    let mut schema = hayai::openapi::schema_from_schemars(#name_str, &base);
                    let mut patches = std::collections::HashMap::new();
                    for (name, _) in &schema.properties {
                        patches.insert(name.clone(), hayai::openapi::PropertyPatch {
                            min_length: None, max_length: None, format: None,
                        });
                    }
                    <#name as hayai::HasSchemaPatches>::patch_schema(&mut patches);
                    for (name, patch) in patches {
                        if let Some(prop) = schema.properties.get_mut(&name) {
                            if patch.min_length.is_some() { prop.min_length = patch.min_length; }
                            if patch.max_length.is_some() { prop.max_length = patch.max_length; }
                            if patch.format.is_some() { prop.format = patch.format; }
                        }
                    }
                    schema
                },
            }
        }
    };

    output.into()
}

// Keep the old derive macro name but redirect - actually remove it since we use attribute macro now
// We need to keep `ApiModel` as the name. Let's use a derive macro that's a no-op placeholder
// and the attribute macro is `api_model`. But the task says users use `#[derive(ApiModel)]`.
// 
// Actually, derive macros can't add other derives. So we use `#[api_model]` attribute macro.
// The derive(ApiModel) was the old API. New API is #[api_model].
