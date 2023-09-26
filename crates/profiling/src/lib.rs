use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, ItemFn};

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
