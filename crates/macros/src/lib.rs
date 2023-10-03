/*  Copyright 2022-23, Juspay India Pvt Ltd
    This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License
    as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version. This program
    is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
    or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details. You should have received a copy of
    the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
*/

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, ItemEnum, ItemFn};

#[proc_macro_attribute]
pub fn measure_duration(_: TokenStream, input: TokenStream) -> TokenStream {
    let input_fn = parse_macro_input!(input as ItemFn);
    let function_body = &input_fn.block;
    let fn_name = &input_fn.sig.ident;
    let args = &input_fn.sig.inputs;
    let return_type = &input_fn.sig.output;

    let expanded = quote! {
        pub async fn #fn_name(#args) #return_type {
            let start_time = std::time::Instant::now();
            let result = #function_body;
            let elapsed_time = start_time.elapsed();
            let elapsed_ms = elapsed_time.as_secs() * 1000 + u64::from(elapsed_time.subsec_millis());
            debug!("Function: {} | Duration (ms): {}", stringify!(#fn_name), elapsed_ms);
            result
        }
    };

    TokenStream::from(expanded)
}

#[proc_macro_attribute]
pub fn generate_flamegraph(_: TokenStream, input: TokenStream) -> TokenStream {
    let input_fn = parse_macro_input!(input as ItemFn);
    let function_body = &input_fn.block;
    let fn_name = &input_fn.sig.ident;
    let args = &input_fn.sig.inputs;
    let return_type = &input_fn.sig.output;
    let visibility = &input_fn.vis;
    let asyncness = input_fn.sig.asyncness;

    let function_start = match (asyncness, visibility) {
        (Some(_), _) => quote! { pub async fn },
        (None, syn::Visibility::Public(_)) => quote! { pub fn },
        (None, _) => quote! { fn },
    };

    let expanded = quote! {
        #function_start #fn_name(#args) #return_type {
            let guard = pprof::ProfilerGuard::new(1000).unwrap();
            let result = #function_body;
            if let Ok(report) = guard.report().build() {
                std::fs::create_dir_all("./profiling").unwrap();
                let flamegraph_file = std::fs::File::create(format!("./profiling/{}-flamegraph.svg", stringify!(#fn_name))).unwrap();
                let mut prof_file = std::fs::File::create(format!("./profiling/{}-profiling.prof", stringify!(#fn_name))).unwrap();
                let _ = report
                    .flamegraph(flamegraph_file)
                    .map_err(|err| err.to_string());
                std::io::Write::write_all(&mut prof_file, format!("{:?}", report).as_bytes()).unwrap();
            };
            result
        }
    };

    TokenStream::from(expanded)
}

#[proc_macro_attribute]
pub fn add_error(_: TokenStream, input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as ItemEnum);
    let enum_name = &input.ident;

    let variants = input.variants.iter().map(|variant| {
        let variant_name = &variant.ident;
        let variant_screaming_snake_case = convert_to_snake_case(variant_name.to_string());
        quote! {
            #[error(#variant_screaming_snake_case)]
            #variant,
        }
    });

    let expanded = quote! {
        #[derive(Debug, Serialize, thiserror::Error)]
        pub enum #enum_name {
            #(#variants)*
        }
    };

    TokenStream::from(expanded)
}

fn convert_to_snake_case(input: String) -> String {
    let mut result = String::new();
    let mut last_char_was_upper = false;

    for c in input.chars() {
        if c.is_uppercase() {
            if !last_char_was_upper && !result.is_empty() {
                result.push('_');
            }
            result.push(c.to_ascii_uppercase());
            last_char_was_upper = true;
        } else {
            result.push(c.to_ascii_uppercase());
            last_char_was_upper = false;
        }
    }

    result
}
