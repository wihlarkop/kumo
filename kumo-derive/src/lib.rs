use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{Data, DeriveInput, Fields, LitStr, Type, parse_macro_input};

/// Derive macro that generates an [`Extract`] implementation for a struct.
///
/// Each field must carry `#[extract(css = "selector")]` plus optional modifiers:
/// - `attr = "name"` — read an HTML attribute instead of text content
/// - `re = r"pattern"` — apply a regex and take the first capture / match
/// - `text` — explicit text content (the default; can be omitted)
/// - `llm_fallback = "hint"` — fall back to LLM when selector returns empty
/// - `llm_fallback` — same, using field name as the extraction hint
///
/// `String` fields use `unwrap_or_default()` on missing matches.
/// `Option<String>` fields stay as `Option` (no unwrap).
///
/// ```rust,ignore
/// #[derive(Extract, Serialize)]
/// struct Book {
///     #[extract(css = "h3 a", attr = "title")]
///     title: String,
///     #[extract(css = ".price_color", llm_fallback = "the price in GBP")]
///     price: String,
/// }
/// ```
#[proc_macro_derive(Extract, attributes(extract))]
pub fn derive_extract(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match impl_extract(&input) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

struct FieldInfo {
    name: syn::Ident,
    is_option: bool,
    args: ExtractArgs,
}

fn impl_extract(input: &DeriveInput) -> syn::Result<TokenStream2> {
    let name = &input.ident;
    let Data::Struct(data) = &input.data else {
        return Err(syn::Error::new_spanned(
            input,
            "#[derive(Extract)] only supports structs",
        ));
    };
    let Fields::Named(fields) = &data.fields else {
        return Err(syn::Error::new_spanned(
            input,
            "#[derive(Extract)] requires named fields",
        ));
    };

    let field_infos: Vec<FieldInfo> = fields
        .named
        .iter()
        .map(|field| {
            Ok(FieldInfo {
                name: field.ident.as_ref().unwrap().clone(),
                is_option: is_option_type(&field.ty),
                args: parse_extract_args(field)?,
            })
        })
        .collect::<syn::Result<Vec<_>>>()?;

    let has_llm_fallback = field_infos.iter().any(|f| f.args.llm_fallback.is_some());

    // Generate per-field sync extraction (as Option<String> for everything).
    let sync_extraction: Vec<TokenStream2> = field_infos
        .iter()
        .map(|fi| {
            let field_name = &fi.name;
            let css = &fi.args.css;
            let base = quote! { element.css(#css).first() };
            let valued = match (&fi.args.attr, &fi.args.re) {
                (Some(attr), _) => quote! { #base.and_then(|e| e.attr(#attr)) },
                (_, Some(re)) => quote! { #base.and_then(|e| e.re_first(#re)) },
                _ => quote! { #base.map(|e| e.text()) },
            };
            let transform_expr = match fi.args.transform.as_ref().map(|t| t.value()) {
                Some(ref t) if t == "trim" => {
                    quote! { .map(|s: String| s.trim().to_string()) }
                }
                Some(ref t) if t == "lowercase" => {
                    quote! { .map(|s: String| s.to_lowercase()) }
                }
                Some(ref t) if t == "uppercase" => {
                    quote! { .map(|s: String| s.to_uppercase()) }
                }
                _ => quote! {},
            };
            let var = quote::format_ident!("__field_{}", field_name);
            quote! { let mut #var: Option<String> = (#valued)#transform_expr; }
        })
        .collect();

    // Generate LLM fallback block (only if any field has llm_fallback).
    let llm_block = if has_llm_fallback {
        // Build the schema properties entries for all llm_fallback fields.
        let schema_entries: Vec<TokenStream2> = field_infos
            .iter()
            .filter_map(|fi| {
                fi.args.llm_fallback.as_ref().map(|hint_opt| {
                    let field_str = fi.name.to_string();
                    let hint = hint_opt
                        .as_ref()
                        .map(|s| s.value())
                        .unwrap_or_else(|| field_str.clone());
                    quote! {
                        props.insert(
                            #field_str.to_string(),
                            ::serde_json::json!({ "type": "string", "description": #hint }),
                        );
                    }
                })
            })
            .collect();

        // Generate the missing-check condition.
        let missing_checks: Vec<TokenStream2> = field_infos
            .iter()
            .filter_map(|fi| {
                if fi.args.llm_fallback.is_some() {
                    let var = quote::format_ident!("__field_{}", fi.name);
                    Some(quote! { #var.as_ref().map(|s| s.trim().is_empty()).unwrap_or(true) })
                } else {
                    None
                }
            })
            .collect();

        // Generate the fill-in assignments after the LLM call.
        let fill_ins: Vec<TokenStream2> = field_infos
            .iter()
            .filter_map(|fi| {
                if fi.args.llm_fallback.is_some() {
                    let field_str = fi.name.to_string();
                    let var = quote::format_ident!("__field_{}", fi.name);
                    Some(quote! {
                        if #var.as_ref().map(|s| s.trim().is_empty()).unwrap_or(true) {
                            #var = __llm_json.get(#field_str)
                                .and_then(|v| v.as_str())
                                .filter(|s| !s.trim().is_empty())
                                .map(|s| s.to_string());
                        }
                    })
                } else {
                    None
                }
            })
            .collect();

        quote! {
            if #(#missing_checks)||* {
                if let Some(__llm_client) = llm {
                    let mut props = ::serde_json::Map::new();
                    #(#schema_entries)*
                    let __schema = ::serde_json::json!({
                        "type": "object",
                        "properties": props
                    });
                    let (__llm_json, _) = __llm_client
                        .extract_json(&__schema, element.outer_html())
                        .await?;
                    #(#fill_ins)*
                }
            }
        }
    } else {
        quote! {}
    };

    // Generate struct construction expressions.
    let struct_fields: Vec<TokenStream2> = field_infos
        .iter()
        .map(|fi| {
            let field_name = &fi.name;
            let var = quote::format_ident!("__field_{}", field_name);
            if fi.is_option {
                quote! { #field_name: #var }
            } else if let Some(default) = &fi.args.default_val {
                quote! { #field_name: #var.unwrap_or_else(|| #default.to_string()) }
            } else {
                quote! { #field_name: #var.unwrap_or_default() }
            }
        })
        .collect();

    Ok(quote! {
        #[::async_trait::async_trait]
        impl ::kumo::extract::Extract for #name {
            async fn extract_from(
                element: &::kumo::extract::Element,
                llm: ::std::option::Option<&dyn ::kumo::llm::client::LlmClient>,
            ) -> ::std::result::Result<Self, ::kumo::error::KumoError> {
                #(#sync_extraction)*
                #llm_block
                ::std::result::Result::Ok(#name {
                    #(#struct_fields),*
                })
            }
        }
    })
}

struct ExtractArgs {
    css: LitStr,
    attr: Option<LitStr>,
    re: Option<LitStr>,
    /// `Some(Some(hint))` = `llm_fallback = "hint"`, `Some(None)` = bare `llm_fallback`.
    llm_fallback: Option<Option<LitStr>>,
    /// Fallback string for `String` fields when the selector returns empty.
    default_val: Option<LitStr>,
    /// Named transform: "trim", "lowercase", or "uppercase".
    transform: Option<LitStr>,
}

fn parse_extract_args(field: &syn::Field) -> syn::Result<ExtractArgs> {
    let attr = field
        .attrs
        .iter()
        .find(|a| a.path().is_ident("extract"))
        .ok_or_else(|| {
            syn::Error::new_spanned(field, "field is missing #[extract(css = \"...\")]")
        })?;

    let mut css: Option<LitStr> = None;
    let mut attr_val: Option<LitStr> = None;
    let mut re_val: Option<LitStr> = None;
    let mut llm_fallback: Option<Option<LitStr>> = None;
    let mut default_val: Option<LitStr> = None;
    let mut transform: Option<LitStr> = None;

    attr.parse_nested_meta(|meta| {
        if meta.path.is_ident("css") {
            css = Some(meta.value()?.parse()?);
        } else if meta.path.is_ident("attr") {
            attr_val = Some(meta.value()?.parse()?);
        } else if meta.path.is_ident("re") {
            re_val = Some(meta.value()?.parse()?);
        } else if meta.path.is_ident("text") {
            // explicit text — no-op, it's the default
        } else if meta.path.is_ident("llm_fallback") {
            if meta.input.peek(syn::Token![=]) {
                let hint: LitStr = meta.value()?.parse()?;
                llm_fallback = Some(Some(hint));
            } else {
                llm_fallback = Some(None);
            }
        } else if meta.path.is_ident("default") {
            default_val = Some(meta.value()?.parse()?);
        } else if meta.path.is_ident("transform") {
            let lit: LitStr = meta.value()?.parse()?;
            let val = lit.value();
            if !matches!(val.as_str(), "trim" | "lowercase" | "uppercase") {
                return Err(syn::Error::new(
                    lit.span(),
                    format!("unknown transform `{val}` — valid values: trim, lowercase, uppercase"),
                ));
            }
            transform = Some(lit);
        } else {
            let key = meta
                .path
                .get_ident()
                .map(|i| i.to_string())
                .unwrap_or_default();
            return Err(meta.error(format!(
                "unknown extract attribute `{key}` — valid keys: css, attr, re, text, llm_fallback, default, transform"
            )));
        }
        Ok(())
    })?;

    let css =
        css.ok_or_else(|| syn::Error::new_spanned(attr, "#[extract] requires css = \"selector\""))?;

    Ok(ExtractArgs {
        css,
        attr: attr_val,
        re: re_val,
        llm_fallback,
        default_val,
        transform,
    })
}

fn is_option_type(ty: &Type) -> bool {
    if let Type::Path(tp) = ty
        && let Some(seg) = tp.path.segments.last()
    {
        return seg.ident == "Option";
    }
    false
}
