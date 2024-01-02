extern crate proc_macro;
use syn::spanned::Spanned;
use proc_macro2::{Ident, TokenStream as TokenStream2};
use proc_macro::{ TokenStream};
use std::any::{Any, TypeId};
use std::ops::{Deref, DerefMut, Index};
use quote::{format_ident, quote_spanned};
use syn::{parse_macro_input, Attribute, ItemFn, Expr, DeriveInput, MetaList, LitInt, FnArg, PatType, Type, ReturnType, ItemStruct};
use syn::parse::Parser;
use syn::token::{Const, Token};
use quote::quote;
// 定义宏，并用在自定义的handler上
// 该宏将拆分函数的参数，并且对每种参数类型实现相应逻辑
#[proc_macro_attribute]
pub fn cs_handler(attrs: TokenStream, input: TokenStream) -> TokenStream {
    let input_fn = parse_macro_input!(input as ItemFn); // 捕获函数定义
    // 假设我们有一个能够针对每种类型调用特定函数的方法 `call_handler_for_type`
    let body = &input_fn.block; // 获取原始函数的函数体
    let fn_name = &input_fn.sig.ident;
    let inputs = &input_fn.sig.inputs; // 获取函数的输入参数
    let ret = &input_fn.sig.output;
    let mut v = Vec::new();
    let mut args_name = Vec::new();
    let arg = "arg";
    for (ii,i) in inputs.iter().enumerate() {
        match i {
            FnArg::Receiver(parse_macro_input) => {}
            FnArg::Typed(PatType{pat,ty,..}) => {
                let new_ide = format_ident!("{}{}",arg,ii);
                let nty = match ty.deref() {
                    Type::Path(p) => {
                        &p.path.segments.first().unwrap().ident
                    }
                    t =>{
                        panic!("invalid args type:{:?}",t)
                    }
                };
                println!("wiuqweq:{:?}",nty);
                args_name.push(new_ide.clone());
                let ss = quote!{
                    let #new_ide : #ty = #nty::from_cs_request(&mut rr);
                };
                v.push(ss);
            }
        }
    }
    let impl_block = quote!{
        #[derive(Debug)]
        pub struct #fn_name;
        #[async_trait]
        impl svr_endpoint::cs::CsHandler for #fn_name {
            async fn handle(&self,mut r: CsRequest) -> CsResponse {
                #input_fn
                let mut rr = r;
                #(#v)*
                #fn_name(#(#args_name,)*).await.into_cs_resp()
            }
        }
    };
    let b = quote!{
        #impl_block
    };
    TokenStream::from(b)
}

fn fn_case_2_struct_case(ident: &Ident) -> Ident {
    let s = ident.to_string();
    // 移除下划线，每个单词首字母大写
    let new_name = s.split('_')
        .map(|word| {
            let mut c = word.chars();
            match c.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            }
        })
        .collect::<String>();

    Ident::new(&new_name, ident.span())
}

#[proc_macro_derive(IntoCsResponse)]
pub fn into_cs_resp(input :TokenStream) -> TokenStream {
    let input_struct = parse_macro_input!(input as ItemStruct); // 捕获函数定义
    let struct_name = input_struct.ident;
    let s_name = format!("{}",struct_name);
    println!("ouwqiwe:{}",s_name);
    if s_name.ends_with("Req") || s_name.ends_with("Resp") {
        let impl_s = quote!{
            impl crate::cs::IntoCsResponse for #struct_name {
                fn into_cs_resp(self) -> crate::cs::CsResponse {
                    use prost::Message;
                    let mut bm = bytes::BytesMut::new();
                    match self.encode(&mut bm).map_err(|e|crate::cs::CsError::Other(format!("{:?}",e))) {
                        Ok(_) => {
                            bm.freeze().into_cs_resp()
                        }
                        Err(e) => {
                            e.into_cs_resp()
                        }
                    }
                }
            }
        };
        return
            TokenStream::from(impl_s)
    }
    TokenStream::from(quote!())
}