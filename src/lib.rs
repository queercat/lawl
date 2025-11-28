use std::{collections::HashMap, fmt::Display, sync::Mutex};

use erased_serde::Serialize;
use lol_html::{HtmlRewriter, Settings, element, html_content::Element};
use mlua::{Lua, LuaSerdeExt};

pub struct Lawl {
    environment: Environment,
}

type Value = Box<dyn Serialize + Sync + Send>;
type Wrapper<T> = Mutex<T>;

impl Lawl {
    pub fn render(&self, html: &impl Display) -> Result<String, ()> {
        html.to_string().render(&self.environment)
    }

    pub fn insert<T: Serialize + Sync + Send + 'static>(
        &mut self,
        key: &impl Display,
        value: T,
    ) -> Result<(), ()> {
        self.environment
            .values
            .insert(key.to_string(), Mutex::new(Box::new(value)));
        Ok(())
    }

    pub fn remove(&mut self, key: &impl Display) -> Result<(), ()> {
        self.environment.values.remove(&key.to_string());
        Ok(())
    }
}

impl Default for Lawl {
    fn default() -> Self {
        Self {
            environment: Default::default(),
        }
    }
}

pub struct Environment {
    pub values: HashMap<String, Wrapper<Value>>,
    pub functions: Vec<String>,
}

impl Default for Environment {
    fn default() -> Self {
        Self {
            values: HashMap::new(),
            functions: vec![
                "function show(v) if (v or '') == '' then data = '' end end".to_string(),
                "function hide(v) if (v or '') ~= '' then data = '' end end".to_string(),
                "function maybe(v, o) return v or o end".to_string(),
                "function format(...) data = string.format(data, ...) end".to_string(),
                "function each(k) local template = data; data = ''; for _, post in ipairs(k) do data = data .. template:gsub('%$([a-zA-Z_]+)', post) end end".to_string()
            ],
        }
    }
}

trait Render {
    fn render(&self, environment: &Environment) -> Result<String, ()>;
}

impl Render for String {
    fn render(&self, environment: &Environment) -> Result<String, ()> {
        let mut env = vec![];
        let lua = Lua::new();

        for (k, v) in &environment.values {
            let value = v.lock().unwrap();

            let value = lua.to_value(&value.as_ref()).unwrap();
            env.push((k.to_owned(), value));
        }

        for v in &environment.functions {
            lua.load(v).exec().unwrap();
        }

        for (k, v) in env {
            lua.globals().set(k, v).expect("Unable to assign globals.")
        }

        render(self, lua)
    }
}

fn render(template: &String, lua: Lua) -> Result<String, ()> {
    let mut buffer = vec![];
    let mut rewriter = HtmlRewriter::new(
        Settings {
            element_content_handlers: vec![element!("lua", |el: &mut Element| {
                let start_location = el.source_location().bytes().end;
                let expression = el.get_attribute("code").unwrap_or("".to_string());
                el.remove();
                if let Some(handlers) = el.end_tag_handlers() {
                    let source = template.clone();
                    let e = expression.clone();
                    let lua = lua.clone();

                    handlers.push(Box::new(move |end| {
                        let end_location = end.source_location().bytes().start;
                        let html = source[start_location..end_location].to_string();

                        lua.globals().set("data", html).unwrap();

                        lua.load(&e)
                            .exec()
                            .expect(format!("Invalid Lua expression. {}", e).as_str());

                        let data: String = lua.globals().get("data").unwrap();

                        end.before(&data, lol_html::html_content::ContentType::Html);

                        Ok(())
                    }));
                }
                Ok(())
            })],
            ..Settings::new()
        },
        |c: &[u8]| buffer.extend_from_slice(c),
    );

    rewriter.write(template.as_bytes()).unwrap();

    Ok(String::from_utf8(buffer).unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_construct() {
        Lawl::default();
    }

    #[test]
    fn should_not_mutate_no_lua_html() {
        let lawl = Lawl::default();

        let html = r#"
            <!doctype html>
            <html>
              <head>
                <title>This is the title of the webpage!</title>
              </head>
              <body>
                <p>This is an example paragraph. Anything in the <strong>body</strong> tag will appear on the page, just like this <strong>p</strong> tag and its contents.</p>
              </body>
            </html>
        "#.to_string();

        assert_eq!(html, lawl.render(&html).unwrap());
    }

    #[test]
    fn should_generate_correct_result_from_basic_lua_expression() {
        let lawl = Lawl::default();

        let html = r#"<Lua code='data = "my little pony"'>replace me!</Lua>"#.to_string();

        debug_assert_eq!("my little pony".to_string(), lawl.render(&html).unwrap())
    }
}
