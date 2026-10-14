//! A very small XML reader: just enough for the parts of office files that hold text.
//! It does not validate; it turns well-formed input into events.

pub(crate) enum Event {
    /// `<name attr="value">`; `<name/>` gives a `Start` immediately followed by an `End`.
    Start {
        name: String,
        attrs: Vec<(String, String)>,
    },
    End(String),
    Text(String),
}

pub(crate) fn attr<'a>(attrs: &'a [(String, String)], key: &str) -> Option<&'a str> {
    attrs
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
}

/// `&amp;`, `&#233;` and friends.
pub(crate) fn unescape(text: &str) -> String {
    if !text.contains('&') {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let Some(end) = rest.find(';').filter(|e| *e <= 10) else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let entity = &rest[1..end];
        let decoded = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ => entity
                .strip_prefix("#x")
                .and_then(|h| u32::from_str_radix(h, 16).ok())
                .or_else(|| entity.strip_prefix('#').and_then(|d| d.parse().ok()))
                .and_then(char::from_u32),
        };
        match decoded {
            Some(c) => {
                out.push(c);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

pub(crate) fn events(xml: &str) -> Vec<Event> {
    let mut out = Vec::new();
    let mut rest = xml;
    while !rest.is_empty() {
        let Some(open) = rest.find('<') else {
            out.push(Event::Text(unescape(rest)));
            break;
        };
        if open > 0 {
            out.push(Event::Text(unescape(&rest[..open])));
        }
        rest = &rest[open..];
        if let Some(body) = rest.strip_prefix("<!--") {
            rest = body.find("-->").map_or("", |e| &body[e + 3..]);
        } else if let Some(body) = rest.strip_prefix("<![CDATA[") {
            let end = body.find("]]>").unwrap_or(body.len());
            out.push(Event::Text(body[..end].to_string()));
            rest = body.get(end + 3..).unwrap_or("");
        } else if rest.starts_with("<?") || rest.starts_with("<!") {
            rest = rest.find('>').map_or("", |e| &rest[e + 1..]);
        } else {
            let Some(end) = tag_end(rest) else { break };
            let tag = &rest[1..end];
            rest = &rest[end + 1..];
            if let Some(name) = tag.strip_prefix('/') {
                out.push(Event::End(name.trim().to_string()));
                continue;
            }
            let (tag, empty) = match tag.strip_suffix('/') {
                Some(tag) => (tag, true),
                None => (tag, false),
            };
            let name_end = tag.find(char::is_whitespace).unwrap_or(tag.len());
            let name = tag[..name_end].to_string();
            out.push(Event::Start {
                name: name.clone(),
                attrs: attributes(&tag[name_end..]),
            });
            if empty {
                out.push(Event::End(name));
            }
        }
    }
    out
}

/// The index of the `>` that closes the tag at the start of `text` (quotes may hide one).
fn tag_end(text: &str) -> Option<usize> {
    let mut quote: Option<char> = None;
    for (i, c) in text.char_indices() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (None, '"' | '\'') => quote = Some(c),
            (None, '>') => return Some(i),
            _ => {}
        }
    }
    None
}

fn attributes(mut text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    loop {
        text = text.trim_start();
        let Some(eq) = text.find('=') else { break };
        let key = text[..eq].trim().to_string();
        let value = text[eq + 1..].trim_start();
        let Some(quote) = value.chars().next().filter(|c| matches!(c, '"' | '\'')) else {
            break;
        };
        let Some(end) = value[1..].find(quote) else {
            break;
        };
        out.push((key, unescape(&value[1..1 + end])));
        text = &value[end + 2..];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn describe(xml: &str) -> Vec<String> {
        events(xml)
            .into_iter()
            .map(|e| match e {
                Event::Start { name, attrs } => format!("<{name} {}>", attrs.len()),
                Event::End(name) => format!("</{name}>"),
                Event::Text(t) => format!("'{t}'"),
            })
            .collect()
    }

    #[test]
    fn tags_text_and_empty_elements() {
        assert_eq!(
            describe(r#"<?xml version="1.0"?><a x="1" y='2'>hi<b/>there</a>"#),
            ["<a 2>", "'hi'", "<b 0>", "</b>", "'there'", "</a>"]
        );
    }

    #[test]
    fn entities_comments_and_cdata() {
        assert_eq!(
            unescape("a &amp; b &lt;c&gt; &#233; &#x105; &bogus; &"),
            "a & b <c> é ą &bogus; &"
        );
        assert_eq!(
            describe("<a><!-- skip --><![CDATA[1 < 2]]></a>"),
            ["<a 0>", "'1 < 2'", "</a>"]
        );
    }

    #[test]
    fn a_greater_sign_inside_an_attribute_does_not_end_the_tag() {
        let events = events(r#"<a title="x>y">t</a>"#);
        match &events[0] {
            Event::Start { attrs, .. } => assert_eq!(attr(attrs, "title"), Some("x>y")),
            _ => panic!("expected a start tag"),
        }
    }

    #[test]
    fn broken_input_does_not_panic() {
        for bad in [
            "<",
            "<a",
            "<a b=",
            "</",
            "&",
            "<!--",
            "<![CDATA[x",
            "<a b='c>",
        ] {
            let _ = events(bad);
        }
    }
}
