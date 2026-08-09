use crate::*;

/// 任务注册表生成输入
pub(crate) struct TaskRegistryRegistrationInput {
    /// 任务注册表变量表达式
    registry_expr: Expr,

    /// 声明式注册项
    entries: Vec<TaskRegistryRegistrationEntry>,
}

/// 任务注册项
pub(crate) enum TaskRegistryRegistrationEntry {
    /// 队列任务
    Queue { definition: Expr, handler: Expr },

    /// 定时任务
    Schedule { definition: Expr, handler: Expr },

    /// 队列任务别名
    QueueAlias { alias: Expr, target: Expr },

    /// 定时任务别名
    ScheduleAlias { alias: Expr, target: Expr },
}

impl Parse for TaskRegistryRegistrationInput {
    /// 解析任务注册表生成配置
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let registry_label = input.parse::<Ident>()?;
        if registry_label != "registry" {
            return Err(syn::Error::new(registry_label.span(), "期望 registry"));
        }
        input.parse::<Token![=]>()?;
        let registry_expr = input.parse::<Expr>()?;
        input.parse::<Token![;]>()?;

        let mut entries = Vec::new();
        while !input.is_empty() {
            entries.push(input.parse::<TaskRegistryRegistrationEntry>()?);
        }

        Ok(Self {
            registry_expr,
            entries,
        })
    }
}

impl Parse for TaskRegistryRegistrationEntry {
    /// 解析单个任务注册项
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let kind = input.parse::<Ident>()?;
        let content;
        braced!(content in input);

        let entry = match kind.to_string().as_str() {
            "queue" => Self::Queue {
                definition: parse_named_expr(&content, "definition")?,
                handler: parse_named_expr(&content, "handler")?,
            },
            "schedule" => Self::Schedule {
                definition: parse_named_expr(&content, "definition")?,
                handler: parse_named_expr(&content, "handler")?,
            },
            "queue_alias" => Self::QueueAlias {
                alias: parse_named_expr(&content, "alias")?,
                target: parse_named_expr(&content, "target")?,
            },
            "schedule_alias" => Self::ScheduleAlias {
                alias: parse_named_expr(&content, "alias")?,
                target: parse_named_expr(&content, "target")?,
            },
            _ => return Err(syn::Error::new(kind.span(), "不支持的任务注册类型")),
        };

        if !content.is_empty() {
            return Err(content.error("任务注册项包含未识别的额外字段"));
        }

        Ok(entry)
    }
}

/// 解析命名表达式字段
pub(crate) fn parse_named_expr(input: ParseStream, expected_name: &str) -> syn::Result<Expr> {
    let field_name = input.parse::<Ident>()?;
    if field_name != expected_name {
        return Err(syn::Error::new(
            field_name.span(),
            format!("期望字段 {expected_name}"),
        ));
    }
    input.parse::<Token![:]>()?;
    let expr = input.parse::<Expr>()?;

    if input.peek(Token![,]) {
        input.parse::<Token![,]>()?;
    }

    Ok(expr)
}

/// 展开任务注册表生成配置
pub(crate) fn expand_task_registry_registration(
    input: TaskRegistryRegistrationInput,
) -> syn::Result<TokenStream> {
    let registry_expr = input.registry_expr;
    let is_direct_path = matches!(strip_transparent_expr(&registry_expr), Expr::Path(_));
    let registry_target = if is_direct_path {
        quote! { (#registry_expr) }
    } else {
        quote! { __task_registry }
    };
    let mut task_statements = Vec::new();
    let mut alias_statements = Vec::new();
    for entry in input.entries {
        match entry {
            TaskRegistryRegistrationEntry::Queue {
                definition,
                handler,
            } => {
                task_statements.push(quote! {
                    #registry_target.register_queue_with_definition(#definition, #handler)?;
                });
            }
            TaskRegistryRegistrationEntry::Schedule {
                definition,
                handler,
            } => {
                task_statements.push(quote! {
                    #registry_target.register_schedule_with_definition(#definition, #handler)?;
                });
            }
            TaskRegistryRegistrationEntry::QueueAlias { alias, target } => {
                alias_statements.push(quote! {
                    #registry_target.register_queue_alias(#alias, #target)?;
                });
            }
            TaskRegistryRegistrationEntry::ScheduleAlias { alias, target } => {
                alias_statements.push(quote! {
                    #registry_target.register_schedule_alias(#alias, #target)?;
                });
            }
        }
    }

    let registry_binding = if is_direct_path {
        quote! {}
    } else {
        quote! {
            let __task_registry = &mut (#registry_expr);
        }
    };

    Ok(quote! {
        {
            (|| -> ::core::result::Result<(), ::std::string::String> {
                #registry_binding
                #(#task_statements)*
                #(#alias_statements)*
                ::core::result::Result::Ok(())
            })()
        }
    }
    .into())
}
