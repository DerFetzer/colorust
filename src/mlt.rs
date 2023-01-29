use std::{collections::HashMap, str::FromStr};

use roxmltree::Node;

use crate::ffmpeg::{Filter, FilterColortemp, FilterEq, FilterExposure, FilterLut};

pub fn get_property_value<T: FromStr>(node: &Node, name: &str) -> Option<T> {
    node.descendants()
        .find(|n| n.tag_name().name() == "property" && n.attribute("name") == Some(name))
        .and_then(|n| {
            n.text().map(|v| {
                // Remove time stamps
                if v.contains('=') {
                    v.splitn(2, '=').last().and_then(|v| v.parse().ok())
                } else {
                    v.parse().ok()
                }
            })
        })
        .flatten()
}

pub fn get_filter_strings(root: &Node) -> HashMap<String, String> {
    let mut filter_strings = HashMap::new();
    for entry in root
        .first_child()
        .unwrap()
        .children()
        .filter(|n| n.has_tag_name("playlist"))
        .flat_map(|n| n.children().filter(|n| n.has_tag_name("entry")))
    {
        let producer = entry.attribute("producer").unwrap();
        let filter_string = entry
            .children()
            .filter(|n| n.has_tag_name("filter"))
            .flat_map(|n| {
                let filter: Option<Box<dyn Filter>> =
                    if let Ok(filter) = TryInto::<FilterLut>::try_into(&n) {
                        Some(Box::new(filter))
                    } else if let Ok(filter) = TryInto::<FilterEq>::try_into(&n) {
                        Some(Box::new(filter))
                    } else if let Ok(filter) = TryInto::<FilterExposure>::try_into(&n) {
                        Some(Box::new(filter))
                    } else if let Ok(filter) = TryInto::<FilterColortemp>::try_into(&n) {
                        Some(Box::new(filter))
                    } else {
                        None
                    };
                filter
                    .filter(|f| f.is_active())
                    .map(|f| f.to_filter_string())
            })
            .collect::<Vec<_>>()
            .join(",");
        if !filter_string.is_empty() {
            filter_strings.insert(
                get_url_from_producer(root, producer).unwrap(),
                filter_string,
            );
        }
    }
    filter_strings
}

fn get_url_from_producer(root: &Node, producer: &str) -> Option<String> {
    Some(
        root.first_child()?
            .children()
            .find(|n| n.has_tag_name("producer") && n.attribute("id").unwrap() == producer)?
            .children()
            .find(|n| {
                n.has_tag_name("property") && n.attribute("name") == Some("kdenlive:originalurl")
            })?
            .text()
            .unwrap()
            .to_string(),
    )
}

pub fn add_filtergraph_to_producers(
    xml: String,
    filter_strings: &HashMap<String, String>,
    delete_existing: bool,
) -> String {
    let mut output = Vec::new();
    for line in xml.lines() {
        if delete_existing && line.contains("name=\"filtergraph\"") {
            continue;
        }
        if line.contains(r#"<property name="kdenlive:originalurl"#) {
            let url = line
                .trim()
                .strip_prefix(r#"<property name="kdenlive:originalurl">"#)
                .unwrap()
                .strip_suffix(r#"</property>"#)
                .unwrap();
            if let Some(filter_string) = filter_strings.get(&url.to_string()) {
                output.push(format!(
                    "  <property name=\"filtergraph\">{filter_string}</property>",
                ));
            }
        }
        output.push(line.to_string());
    }
    output.join("\n")
}

#[cfg(test)]
mod tests {
    use roxmltree::Document;

    use super::*;

    #[test]
    fn properties() {
        let xml = r#"
               <filter id="filter6">
                <property name="mlt_service">avfilter.exposure</property>
                <property name="kdenlive_id">avfilter.exposure</property>
                <property name="av.exposure">00:00:00.000=0</property>
                <property name="av.black">00:00:00.000=0</property>
                <property name="kdenlive:collapsed">1</property>
                <property name="disable">1</property>
               </filter>
            "#;
        let doc = Document::parse(xml).unwrap();
        let root = doc.root();

        assert_eq!(
            get_property_value(&root, "mlt_service"),
            Some("avfilter.exposure".to_string())
        );
        assert_eq!(get_property_value(&root, "av.exposure"), Some(0.0));
    }
}
