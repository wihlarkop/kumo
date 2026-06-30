use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{
    Data, DeriveInput, Fields, GenericArgument, LitStr, Path, PathArguments, Type,
    parse_macro_input,
};

/// Derive macro that generates an [`Extract`] implementation for a struct.
///
/// Each field must carry `#[extract(css = "selector")]` plus optional modifiers:
/// - `attr = "name"` - read an HTML attribute instead of text content
/// - `re = r"pattern"` - apply a regex and take the first capture / match
/// - `text` - explicit text content (the default; can be omitted)
/// - `llm_fallback = "hint"` - fall back to LLM when selector returns empty
/// - `llm_fallback` - same, using field name as the extraction hint
///
/// `llm_fallback` is not supported on `Vec<T>` fields or together with `default`.
///
/// `String` fields use `unwrap_or_default()` on missing matches.
/// `Option<T>` fields stay as `Option` (no unwrap).
/// `Vec<T>` fields collect all selector matches.
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
    ty: Type,
    kind: FieldKind,
    args: ExtractArgs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FieldKind {
    Required(ValueKind),
    Optional(ValueKind),
    Vec(ValueKind),
    Nested,
    OptionalNested,
    VecNested,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ValueKind {
    String,
    Scalar(ScalarKind),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScalarKind {
    Bool,
    Int,
    Float,
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
                ty: field.ty.clone(),
                kind: field_kind(&field.ty)?,
                args: parse_extract_args(field)?,
            })
        })
        .collect::<syn::Result<Vec<_>>>()?;

    for fi in &field_infos {
        if fi.args.llm_fallback.is_some() && fi.args.default_val.is_some() {
            return Err(syn::Error::new(
                fi.name.span(),
                "llm_fallback cannot be combined with default; fallback chains are not supported",
            ));
        }
        if matches!(fi.kind, FieldKind::Vec(_)) && fi.args.llm_fallback.is_some() {
            return Err(syn::Error::new(
                fi.name.span(),
                "llm_fallback is not supported for Vec<T> fields",
            ));
        }
        if matches!(
            fi.kind,
            FieldKind::Nested | FieldKind::OptionalNested | FieldKind::VecNested
        ) && (fi.args.attr.is_some()
            || fi.args.re.is_some()
            || fi.args.llm_fallback.is_some()
            || fi.args.default_val.is_some()
            || fi.args.transform.is_some())
        {
            return Err(syn::Error::new(
                fi.name.span(),
                "nested Extract fields only support css; remove attr, re, default, transform, and llm_fallback",
            ));
        }
    }

    let has_llm_fallback = field_infos.iter().any(|f| f.args.llm_fallback.is_some());

    // Generate per-field sync extraction as raw strings.
    let sync_extraction: Vec<TokenStream2> = field_infos
        .iter()
        .map(|fi| {
            let field_name = &fi.name;
            let css = &fi.args.css;
            let raw_for_element = match &fi.args.attr {
                Some(attr) => quote! { __el.attr(#attr) },
                None => quote! { ::std::option::Option::Some(__el.text()) },
            };
            let valued_for_element = match &fi.args.re {
                Some(re) => quote! {
                    (#raw_for_element).and_then(|s| {
                        <::kumo::extract::RegexExtractor as ::kumo::extract::ValueExtractor>
                            ::extract_values(&::kumo::extract::RegexExtractor, &s, #re)
                            .ok()
                            .and_then(|values| values.into_iter().next())
                    })
                },
                None => raw_for_element,
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
            match fi.kind {
                FieldKind::Vec(_) => quote! {
                    let #var: ::std::vec::Vec<String> = element
                        .css(#css)
                        .iter()
                        .filter_map(|__el| (#valued_for_element)#transform_expr)
                        .collect();
                },
                FieldKind::Required(_) | FieldKind::Optional(_)
                    if fi.args.llm_fallback.is_some() =>
                {
                    quote! {
                        let mut #var: ::std::option::Option<String> = element
                            .css(#css)
                            .first()
                            .and_then(|__el| (#valued_for_element))
                            #transform_expr;
                    }
                }
                FieldKind::Required(_) | FieldKind::Optional(_) => quote! {
                    let #var: ::std::option::Option<String> = element
                        .css(#css)
                        .first()
                        .and_then(|__el| (#valued_for_element))
                        #transform_expr;
                },
                FieldKind::Nested | FieldKind::OptionalNested | FieldKind::VecNested => quote! {},
            }
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
            match fi.kind {
                FieldKind::Required(ValueKind::String) => {
                    if let Some(default) = &fi.args.default_val {
                        quote! { #field_name: #var.unwrap_or_else(|| #default.to_string()) }
                    } else {
                        quote! { #field_name: #var.unwrap_or_default() }
                    }
                }
                FieldKind::Optional(ValueKind::String) => quote! { #field_name: #var },
                FieldKind::Vec(ValueKind::String) => quote! { #field_name: #var },
                FieldKind::Required(ValueKind::Scalar(_)) => {
                    let type_name = type_name(&fi.ty);
                    let default = fi.args.default_val.as_ref().map(|default| {
                        quote! { .or_else(|| ::std::option::Option::Some(#default.to_string())) }
                    });
                    quote! {
                        #field_name: {
                            let __raw = #var
                                #default
                                .ok_or_else(|| ::kumo::error::KumoError::parse_msg(
                                    ::std::format!(
                                        "missing required field `{}` for {}",
                                        ::std::stringify!(#field_name),
                                        #type_name,
                                    )
                                ))?;
                            __raw.parse().map_err(|__err| {
                                ::kumo::error::KumoError::parse_msg(::std::format!(
                                    "failed to parse field `{}` as {} from {:?}: {}",
                                    ::std::stringify!(#field_name),
                                    #type_name,
                                    __raw,
                                    __err,
                                ))
                            })?
                        }
                    }
                }
                FieldKind::Optional(ValueKind::Scalar(_)) => {
                    let type_name = option_inner_type_name(&fi.ty);
                    quote! {
                        #field_name: match #var {
                            ::std::option::Option::Some(__raw) => {
                                ::std::option::Option::Some(__raw.parse().map_err(|__err| {
                                    ::kumo::error::KumoError::parse_msg(::std::format!(
                                        "failed to parse field `{}` as {} from {:?}: {}",
                                        ::std::stringify!(#field_name),
                                        #type_name,
                                        __raw,
                                        __err,
                                    ))
                                })?)
                            }
                            ::std::option::Option::None => ::std::option::Option::None,
                        }
                    }
                }
                FieldKind::Vec(ValueKind::Scalar(_)) => {
                    let type_name = vec_inner_type_name(&fi.ty);
                    quote! {
                        #field_name: #var
                            .into_iter()
                            .map(|__raw| {
                                __raw.parse().map_err(|__err| {
                                    ::kumo::error::KumoError::parse_msg(::std::format!(
                                        "failed to parse field `{}` as {} from {:?}: {}",
                                        ::std::stringify!(#field_name),
                                        #type_name,
                                        __raw,
                                        __err,
                                    ))
                                })
                            })
                            .collect::<::std::result::Result<_, ::kumo::error::KumoError>>()?
                    }
                }
                FieldKind::Nested => {
                    let css = &fi.args.css;
                    let ty = &fi.ty;
                    quote! {
                        #field_name: {
                            let __nested_elements = element.css(#css);
                            let __nested = __nested_elements
                                .first()
                                .ok_or_else(|| ::kumo::error::KumoError::parse_msg(
                                    ::std::format!(
                                        "missing required nested field `{}`",
                                        ::std::stringify!(#field_name),
                                    )
                                ))?;
                            <#ty as ::kumo::extract::Extract>::extract_from(__nested, llm).await?
                        }
                    }
                }
                FieldKind::OptionalNested => {
                    let css = &fi.args.css;
                    let inner_ty = container_inner_type(&fi.ty);
                    quote! {
                        #field_name: {
                            let __nested_elements = element.css(#css);
                            match __nested_elements.first() {
                            ::std::option::Option::Some(__nested) => {
                                ::std::option::Option::Some(
                                    <#inner_ty as ::kumo::extract::Extract>::extract_from(__nested, llm).await?
                                )
                            }
                            ::std::option::Option::None => ::std::option::Option::None,
                            }
                        }
                    }
                }
                FieldKind::VecNested => {
                    let css = &fi.args.css;
                    let inner_ty = container_inner_type(&fi.ty);
                    quote! {
                        #field_name: {
                            let mut __nested_items = ::std::vec::Vec::new();
                            let __nested_elements = element.css(#css);
                            for __nested in __nested_elements.iter() {
                                __nested_items.push(
                                    <#inner_ty as ::kumo::extract::Extract>::extract_from(__nested, llm).await?
                                );
                            }
                            __nested_items
                        }
                    }
                }
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
    let attrs = field
        .attrs
        .iter()
        .filter(|a| a.path().is_ident("extract"))
        .collect::<Vec<_>>();
    if attrs.len() > 1 {
        return Err(syn::Error::new_spanned(
            field,
            "field has multiple #[extract(...)] attributes; combine them into one",
        ));
    }

    let attr = attrs.first().copied().ok_or_else(|| {
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
            // explicit text - no-op, it's the default
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
                    format!(
                        "unknown transform `{val}` - valid values: trim, lowercase, uppercase"
                    ),
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
                "unknown extract attribute `{key}` - valid keys: css, attr, re, text, llm_fallback, default, transform"
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

fn field_kind(ty: &Type) -> syn::Result<FieldKind> {
    if let Some(kind) = value_kind(ty) {
        return Ok(FieldKind::Required(kind));
    }

    if let Some(inner) = container_inner(
        ty,
        &[
            &["Option"],
            &["std", "option", "Option"],
            &["core", "option", "Option"],
        ],
    ) && let Some(kind) = value_kind(inner)
    {
        return Ok(FieldKind::Optional(kind));
    }
    if let Some(inner) = container_inner(
        ty,
        &[
            &["Option"],
            &["std", "option", "Option"],
            &["core", "option", "Option"],
        ],
    ) && is_nested_extract_type(inner)
    {
        return Ok(FieldKind::OptionalNested);
    }

    if let Some(inner) = container_inner(
        ty,
        &[&["Vec"], &["std", "vec", "Vec"], &["alloc", "vec", "Vec"]],
    ) && let Some(kind) = value_kind(inner)
    {
        return Ok(FieldKind::Vec(kind));
    }
    if let Some(inner) = container_inner(
        ty,
        &[&["Vec"], &["std", "vec", "Vec"], &["alloc", "vec", "Vec"]],
    ) && is_nested_extract_type(inner)
    {
        return Ok(FieldKind::VecNested);
    }

    if is_nested_extract_type(ty) {
        return Ok(FieldKind::Nested);
    }

    Err(syn::Error::new_spanned(
        ty,
        "unsupported field type; #[derive(Extract)] supports String, bool, numeric primitives, nested Extract structs, Option<T>, and Vec<T> using Rust prelude or canonical std/core/alloc paths",
    ))
}

fn value_kind(ty: &Type) -> Option<ValueKind> {
    if let Type::Path(tp) = ty
        && tp.qself.is_none()
        && let Some(seg) = tp.path.segments.last()
        && tp
            .path
            .segments
            .iter()
            .all(|seg| matches!(seg.arguments, PathArguments::None))
    {
        let name = seg.ident.to_string();
        if path_is_one_of(
            &tp.path,
            &[
                &["String"],
                &["std", "string", "String"],
                &["alloc", "string", "String"],
            ],
        ) {
            return Some(ValueKind::String);
        }
        if name == "bool" && is_primitive_path(&tp.path, &name) {
            return Some(ValueKind::Scalar(ScalarKind::Bool));
        }
        if matches!(
            name.as_str(),
            "i8" | "i16"
                | "i32"
                | "i64"
                | "i128"
                | "isize"
                | "u8"
                | "u16"
                | "u32"
                | "u64"
                | "u128"
                | "usize"
        ) && is_primitive_path(&tp.path, &name)
        {
            return Some(ValueKind::Scalar(ScalarKind::Int));
        }
        if matches!(name.as_str(), "f32" | "f64") && is_primitive_path(&tp.path, &name) {
            return Some(ValueKind::Scalar(ScalarKind::Float));
        }
    }
    None
}

fn container_inner<'a>(ty: &'a Type, paths: &[&[&str]]) -> Option<&'a Type> {
    let Type::Path(tp) = ty else {
        return None;
    };
    if tp.qself.is_some() || !path_is_one_of(&tp.path, paths) {
        return None;
    }

    let last = tp.path.segments.last()?;
    if !tp
        .path
        .segments
        .iter()
        .take(tp.path.segments.len() - 1)
        .all(|seg| matches!(seg.arguments, PathArguments::None))
    {
        return None;
    }

    let PathArguments::AngleBracketed(args) = &last.arguments else {
        return None;
    };
    if args.args.len() != 1 {
        return None;
    }
    match args.args.first()? {
        GenericArgument::Type(inner) => Some(inner),
        _ => None,
    }
}

fn is_nested_extract_type(ty: &Type) -> bool {
    let Type::Path(tp) = ty else {
        return false;
    };
    if tp.qself.is_some()
        || !tp
            .path
            .segments
            .iter()
            .all(|seg| matches!(seg.arguments, PathArguments::None))
    {
        return false;
    }
    let Some(last) = tp.path.segments.last() else {
        return false;
    };
    if let Some(first) = tp.path.segments.first()
        && matches!(first.ident.to_string().as_str(), "std" | "core" | "alloc")
    {
        return false;
    }
    !matches!(
        last.ident.to_string().as_str(),
        "String"
            | "str"
            | "bool"
            | "i8"
            | "i16"
            | "i32"
            | "i64"
            | "i128"
            | "isize"
            | "u8"
            | "u16"
            | "u32"
            | "u64"
            | "u128"
            | "usize"
            | "f32"
            | "f64"
            | "Option"
            | "Vec"
    )
}

fn is_primitive_path(path: &Path, name: &str) -> bool {
    path_has_segments(path, &[name])
        || path_has_segments(path, &["std", "primitive", name])
        || path_has_segments(path, &["core", "primitive", name])
}

fn path_is_one_of(path: &Path, candidates: &[&[&str]]) -> bool {
    candidates
        .iter()
        .any(|segments| path_has_segments(path, segments))
}

fn path_has_segments(path: &Path, expected: &[&str]) -> bool {
    path.segments.len() == expected.len()
        && path
            .segments
            .iter()
            .zip(expected)
            .all(|(segment, expected)| segment.ident == *expected)
}

fn type_name(ty: &Type) -> String {
    if let Type::Path(tp) = ty
        && tp.qself.is_none()
        && let Some(seg) = tp.path.segments.last()
    {
        return seg.ident.to_string();
    }
    quote!(#ty).to_string()
}

fn option_inner_type_name(ty: &Type) -> String {
    type_argument_name(ty, "Option").unwrap_or_else(|| type_name(ty))
}

fn vec_inner_type_name(ty: &Type) -> String {
    type_argument_name(ty, "Vec").unwrap_or_else(|| type_name(ty))
}

fn type_argument_name(ty: &Type, container: &str) -> Option<String> {
    if let Type::Path(tp) = ty
        && tp.qself.is_none()
        && let Some(seg) = tp.path.segments.last()
        && seg.ident == container
        && let PathArguments::AngleBracketed(args) = &seg.arguments
        && args.args.len() == 1
        && let Some(GenericArgument::Type(inner)) = args.args.first()
    {
        return Some(type_name(inner));
    }
    None
}

fn container_inner_type(ty: &Type) -> &Type {
    if let Type::Path(tp) = ty
        && let Some(seg) = tp.path.segments.last()
        && let PathArguments::AngleBracketed(args) = &seg.arguments
        && args.args.len() == 1
        && let Some(GenericArgument::Type(inner)) = args.args.first()
    {
        return inner;
    }
    ty
}
