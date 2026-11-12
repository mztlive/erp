//! 单元测试中的 BSON 管道解释器。未知操作立即失败，不访问数据库。
//! 仅用于对比固定样本的成员判断，不能证明 MongoDB 执行、索引或事务行为。
use std::collections::BTreeMap;

use mongodb::bson::{Bson, Document};

pub(crate) fn execute(
    mut rows: Vec<Document>,
    stages: &[Document],
    collections: &BTreeMap<String, Vec<Document>>,
    variables: &Document,
) -> Vec<Document> {
    for stage in stages {
        validate_variables(&Bson::Document(stage.clone()));
        let (operator, argument) = stage.iter().next().unwrap();
        match operator.as_str() {
            "$match" => rows.retain(|row| matches(argument.as_document().unwrap(), row, variables)),
            "$lookup" => {
                let lookup = argument.as_document().unwrap();
                for row in &mut rows {
                    let mut bindings = variables.clone();
                    for (key, expression) in lookup.get_document("let").unwrap() {
                        bindings.insert(key, value(expression, row, variables));
                    }
                    let stages = lookup
                        .get_array("pipeline")
                        .unwrap()
                        .iter()
                        .map(|stage| stage.as_document().unwrap().clone())
                        .collect::<Vec<_>>();
                    let input = collections.get(lookup.get_str("from").unwrap()).cloned().unwrap_or_default();
                    let joined = execute(input, &stages, collections, &bindings);
                    row.insert(lookup.get_str("as").unwrap(), joined);
                }
            },
            "$unset" => {
                for row in &mut rows {
                    for field in argument.as_array().unwrap() {
                        row.remove(field.as_str().unwrap());
                    }
                }
            },
            "$project" => {
                rows = rows
                    .into_iter()
                    .map(|row| {
                        argument
                            .as_document()
                            .unwrap()
                            .iter()
                            .filter_map(|(field, expression)| {
                                if expression == &Bson::Int32(0) {
                                    return None;
                                }
                                let value = if expression == &Bson::Int32(1) {
                                    path(&row, field)?
                                } else {
                                    value(expression, &row, variables)
                                };
                                Some((field.clone(), value))
                            })
                            .collect()
                    })
                    .collect();
            },
            "$limit" => rows.truncate(usize::try_from(argument.as_i32().unwrap()).unwrap()),
            other => panic!("unsupported fixture stage: {other}"),
        }
    }
    rows
}

/// 按 MongoDB 用户变量规则校验生成的管道，空数据也必须暴露非法 let 名称。
fn validate_variables(value: &Bson) {
    match value {
        Bson::Array(items) => items.iter().for_each(validate_variables),
        Bson::Document(document) => {
            for operator in ["$lookup", "$let"] {
                if let Ok(binding) = document.get_document(operator) {
                    let field = if operator == "$lookup" { "let" } else { "vars" };
                    if let Ok(variables) = binding.get_document(field) {
                        for name in variables.keys() {
                            let mut chars = name.chars();
                            assert!(
                                chars.next().is_some_and(|c| c.is_ascii_lowercase() || !c.is_ascii())
                                    && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || !c.is_ascii()),
                                "invalid MongoDB user variable: {name}"
                            );
                        }
                    }
                }
            }
            document.values().for_each(validate_variables);
        },
        _ => {},
    }
}

fn path(row: &Document, field: &str) -> Option<Bson> {
    let (head, tail) = field.split_once('.').unwrap_or((field, ""));
    let value = row.get(head)?;
    if tail.is_empty() {
        return Some(value.clone());
    }
    match value {
        Bson::Document(row) => path(row, tail),
        Bson::Array(items) => match tail.parse::<usize>() {
            Ok(index) => items.get(index).cloned(),
            Err(_) => Some(items.iter().filter_map(|item| path(item.as_document().unwrap(), tail)).collect()),
        },
        _ => None,
    }
}

fn matches(filter: &Document, row: &Document, variables: &Document) -> bool {
    filter.iter().all(|(field, expected)| match field.as_str() {
        "$and" => expected
            .as_array()
            .unwrap()
            .iter()
            .all(|filter| matches(filter.as_document().unwrap(), row, variables)),
        "$or" => expected
            .as_array()
            .unwrap()
            .iter()
            .any(|filter| matches(filter.as_document().unwrap(), row, variables)),
        "$expr" => value(expected, row, variables).as_bool().unwrap(),
        _ => {
            assert!(!field.starts_with('$'), "unsupported fixture filter: {field}");
            if let Some(operator) = expected.as_document() {
                if let Ok(exists) = operator.get_bool("$exists") {
                    return path(row, field).is_some() == exists;
                }
                let members = operator.get_array("$in").expect("unsupported fixture field operation");
                return path(row, field).is_some_and(|actual| members.contains(&actual));
            }
            path(row, field).as_ref() == Some(expected)
        },
    })
}

fn value(expression: &Bson, row: &Document, variables: &Document) -> Bson {
    match expression {
        Bson::String(field) if field.starts_with("$$") => path(variables, &field[2..]).unwrap_or(Bson::Null),
        Bson::String(field) if field.starts_with('$') => path(row, &field[1..]).unwrap_or(Bson::Null),
        Bson::Array(items) => items.iter().map(|item| value(item, row, variables)).collect(),
        Bson::Document(expression) => {
            assert_eq!(expression.len(), 1);
            let (operator, arguments) = expression.iter().next().unwrap();
            if operator == "$literal" {
                return arguments.clone();
            }
            if operator == "$size" {
                return i32::try_from(value(arguments, row, variables).as_array().unwrap().len())
                    .unwrap()
                    .into();
            }
            if operator == "$map" {
                let mapping = arguments.as_document().unwrap();
                let input = value(mapping.get("input").unwrap(), row, variables);
                return input
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|item| {
                        let mut bound = variables.clone();
                        bound.insert(mapping.get_str("as").unwrap(), item.clone());
                        value(mapping.get("in").unwrap(), row, &bound)
                    })
                    .collect();
            }
            let arguments = arguments
                .as_array()
                .unwrap()
                .iter()
                .map(|item| value(item, row, variables))
                .collect::<Vec<_>>();
            match operator.as_str() {
                "$and" => arguments.iter().all(|value| value.as_bool().unwrap()).into(),
                "$or" => arguments.iter().any(|value| value.as_bool().unwrap()).into(),
                "$eq" => (arguments[0] == arguments[1]).into(),
                "$gt" => (arguments[0].as_i32().unwrap() > arguments[1].as_i32().unwrap()).into(),
                "$in" => arguments[1].as_array().unwrap().contains(&arguments[0]).into(),
                "$anyElementTrue" => {
                    arguments[0].as_array().unwrap().iter().any(|item| item.as_bool().unwrap()).into()
                },
                "$ifNull" => {
                    if arguments[0] == Bson::Null {
                        arguments[1].clone()
                    } else {
                        arguments[0].clone()
                    }
                },
                "$setUnion" => {
                    let mut result = Vec::new();
                    for array in arguments {
                        for item in array.as_array().unwrap() {
                            if !result.contains(item) {
                                result.push(item.clone());
                            }
                        }
                    }
                    result.into()
                },
                "$setEquals" => {
                    let left = arguments[0].as_array().unwrap();
                    let right = arguments[1].as_array().unwrap();
                    (left.iter().all(|item| right.contains(item))
                        && right.iter().all(|item| left.contains(item)))
                    .into()
                },
                other => panic!("unsupported fixture expression: {other}"),
            }
        },
        value => value.clone(),
    }
}

#[cfg(test)]
mod tests {
    use mongodb::bson::doc;

    use super::*;

    #[test]
    #[should_panic(expected = "invalid MongoDB user variable")]
    fn invalid_lookup_variable_fails_even_without_rows() {
        execute(
            vec![],
            &[doc! { "$lookup": {
                "from": "sources", "let": { "__invalid": "$id" }, "pipeline": [], "as": "found"
            } }],
            &BTreeMap::new(),
            &doc! {},
        );
    }
}
