use std::path::Path;

const ZOD_BUNDLE: &str = include_str!("../js/bundle.js");

pub fn run(path: &Path) -> anyhow::Result<String> {
    run_source(&std::fs::read_to_string(path)?)
}

pub fn run_source(user_code: &str) -> anyhow::Result<String> {
    let rt = rquickjs::Runtime::new()?;
    let ctx = rquickjs::Context::full(&rt)?;

    ctx.with(|ctx| {
        // Load the zod bundle (sets globalThis.z)
        if let Err(e) = ctx.eval::<(), _>(ZOD_BUNDLE) {
            return Err(anyhow::anyhow!("Failed to load Zod bundle: {}", js_error_message(&ctx, e)));
        }

        // Load user code: replace `export default` with assignment to globalThis.__schema
        let wrapped = user_code.replace("export default", "globalThis.__schema =");
        if let Err(e) = ctx.eval::<(), _>(wrapped.as_str()) {
            return Err(anyhow::anyhow!("Schema JS error: {}", js_error_message(&ctx, e)));
        }

        // Convert to JSON Schema
        match ctx.eval::<String, _>("JSON.stringify(z.toJSONSchema(globalThis.__schema))") {
            Ok(result) => Ok(result),
            Err(e) => Err(anyhow::anyhow!("toJSONSchema error: {}", js_error_message(&ctx, e))),
        }
    })
}

fn js_error_message(ctx: &rquickjs::Ctx<'_>, error: rquickjs::Error) -> String {
    match error {
        rquickjs::Error::Exception => {
            let exception = ctx.catch();
            if let Some(exc) = exception.as_exception() {
                let msg = exc.message().unwrap_or_default();
                if let Some(stack) = exc.stack() {
                    format!("{msg}\n{stack}")
                } else {
                    msg
                }
            } else {
                format!("{exception:?}")
            }
        }
        other => format!("{other}"),
    }
}
