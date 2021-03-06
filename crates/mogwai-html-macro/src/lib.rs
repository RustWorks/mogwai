use proc_macro2::Span;
use quote::quote;
use syn::{spanned::Spanned, Error};
use syn_rsx::{Node, NodeName, NodeType};


fn node_name_span(name: &NodeName) -> Span {
    match name {
        NodeName::Path(expr_path) => expr_path.span(),
        NodeName::Dash(dash) => dash.span(),
        NodeName::Colon(colon) => colon.span(),
    }
}


fn attribute_to_token_stream(node: Node) -> Result<proc_macro2::TokenStream, Error> {
    let span: Span = node
        .name
        .as_ref()
        .map(node_name_span)
        .unwrap_or(Span::call_site());
    if let Some(key) = node.name_as_string() {
        if let Some(expr) = node.value {
            match key.split(':').collect::<Vec<_>>().as_slice() {
                ["style", name] => Ok(quote! {
                    .style(#name, #expr)
                }),
                ["on", event] => Ok(quote! {
                    .on(#event, #expr)
                }),
                ["window", event] => Ok(quote! {
                    .window_on(#event, #expr)
                }),
                ["document", event] => Ok(quote! {
                    .document_on(#event, #expr)
                }),
                ["post", "build"] => Ok(quote! {
                    .post_build(#expr)
                }),
                [attribute_name] => Ok(quote! {
                    .attribute(#attribute_name, #expr)
                }),
                keys => Err(Error::new(
                    span,
                    format!(
                        "expected `style:*`, `on:*`, `window:*`, `document:*`, `post:build` or \
                         other valid attribute key/value pair. Got `{}`",
                        keys.join(":")
                    ),
                )),
            }
        } else {
            Ok(quote! {
                .boolean_attribute(#key, true)
            })
        }
    } else {
        Err(Error::new(span, "dom attribute is missing a name"))
    }
}


fn partition_unzip<T, F>(items: Vec<T>, f: F) -> (Vec<proc_macro2::TokenStream>, Vec<Error>)
where
    F: Fn(T) -> Result<proc_macro2::TokenStream, Error>,
{
    let (tokens, errs): (Vec<Result<_, _>>, _) = items.into_iter().map(f).partition(Result::is_ok);
    let tokens = tokens
        .into_iter()
        .filter_map(Result::ok)
        .collect::<Vec<_>>();
    let errs = errs.into_iter().filter_map(Result::err).collect::<Vec<_>>();
    (tokens, errs)
}


fn combine_errors(errs: Vec<Error>) -> Option<Error> {
    errs.into_iter()
        .fold(None, |may_prev_error: Option<Error>, err| {
            if let Some(mut prev_error) = may_prev_error {
                prev_error.combine(err);
                Some(prev_error)
            } else {
                Some(err)
            }
        })
}


fn node_to_token_stream(node: Node) -> Result<proc_macro2::TokenStream, Error> {
    match node.node_type {
        NodeType::Element => {
            if let Some(tag) = node.name_as_string() {
                let type_is = match tag.as_str() {
                    "input" => quote! { web_sys::HtmlInputElement },
                    _ => quote! { web_sys::HtmlElement },
                };
                let mut errs: Vec<Error> = vec![];

                let (attribute_tokens, attribute_errs) =
                    partition_unzip(node.attributes, attribute_to_token_stream);
                errs.extend(attribute_errs);

                let (child_tokens, child_errs) =
                    partition_unzip(node.children, node_to_token_stream);
                let child_tokens = child_tokens
                    .into_iter()
                    .map(|child| quote! { .with(#child) });
                errs.extend(child_errs);

                let may_error = combine_errors(errs);
                if let Some(error) = may_error {
                    Err(error)
                } else {
                    Ok(quote! {
                        (mogwai::gizmo::dom::DomWrapper::element(#tag) as DomWrapper<#type_is>)
                           #(#attribute_tokens)*
                           #(#child_tokens)*

                    })
                }
            } else {
                Err(Error::new(Span::call_site(), "node is missing a name"))
            }
        }
        NodeType::Text | NodeType::Block => {
            if let Some(value) = node.value {
                Ok(quote! {#value})
            } else {
                Err(Error::new(Span::call_site(), "dom child node value error"))
            }
        }

        _ => Err(Error::new(
            Span::call_site(),
            "attribute in unsupported position",
        )),
    }
}


#[proc_macro]
pub fn dom(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    match syn_rsx::parse(input, None) {
        Ok(parsed) => {
            let (tokens, errs) = partition_unzip(parsed, node_to_token_stream);
            if let Some(error) = combine_errors(errs) {
                error.to_compile_error().into()
            } else {
                proc_macro::TokenStream::from(match tokens.len() {
                    0 => quote! { compile_error("dom! must not be empty") },
                    1 => {
                        let ts = &tokens[0];
                        quote! { #ts }
                    }
                    _ => quote! { vec![#(#tokens),*] },
                })
            }
        }
        Err(msg) => {
            let msg = format!("{}", msg);
            proc_macro::TokenStream::from(quote! {
                compile_error!(#msg)
            })
        }
    }
}


#[proc_macro]
pub fn target_arch_is_wasm32(_: proc_macro::TokenStream) -> proc_macro::TokenStream {
    proc_macro::TokenStream::from(quote! {
        cfg!(target_arch = "wasm32")
    })
}


#[cfg(test)]
mod ssr_tests {
    use std::str::FromStr;

    #[test]
    fn can_parse_rust_closure() {
        let expr: syn::Expr = syn::parse_str(r#"|i:i32| format!("{}", i)"#).unwrap();
        match expr {
            syn::Expr::Closure(_) => {}
            _ => panic!("wrong expr parse, expected closure"),
        }
    }

    #[test]
    fn can_token_stream_from_string() {
        let _ts = proc_macro2::TokenStream::from_str(r#"|i:i32| format!("{}", i)"#).unwrap();
    }

    #[test]
    fn can_parse_from_token_stream() {
        let _ts = proc_macro2::TokenStream::from_str(r#"<div class="any_class" />"#).unwrap();
    }
}
