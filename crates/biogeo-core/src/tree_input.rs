use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fmt;

use crate::newick::{NewickError, NewickParseOptions, ParsedNewickTree, parse_newick_with_options};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TreeInputFormat {
    Newick,
    Nexus,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ParsedTreeInput {
    pub format: TreeInputFormat,
    pub tree_name: Option<String>,
    pub parsed_tree: ParsedNewickTree,
}

pub fn parse_tree_input(input: &str) -> Result<ParsedTreeInput, TreeInputError> {
    parse_tree_input_with_options(input, NewickParseOptions::default())
}

pub fn parse_tree_input_with_options(
    input: &str,
    options: NewickParseOptions,
) -> Result<ParsedTreeInput, TreeInputError> {
    parse_tree_input_selected_with_options(input, None, options)
}

pub fn parse_tree_input_named(
    input: &str,
    tree_name: &str,
) -> Result<ParsedTreeInput, TreeInputError> {
    parse_tree_input_named_with_options(input, tree_name, NewickParseOptions::default())
}

pub fn parse_tree_input_named_with_options(
    input: &str,
    tree_name: &str,
    options: NewickParseOptions,
) -> Result<ParsedTreeInput, TreeInputError> {
    parse_tree_input_selected_with_options(input, Some(tree_name), options)
}

fn parse_tree_input_selected_with_options(
    input: &str,
    tree_name: Option<&str>,
    options: NewickParseOptions,
) -> Result<ParsedTreeInput, TreeInputError> {
    if has_nexus_header(input) {
        parse_nexus_tree(input, tree_name, options).map_err(TreeInputError::Nexus)
    } else {
        if let Some(tree_name) = tree_name {
            return Err(TreeInputError::TreeNameForNewick {
                tree_name: tree_name.to_string(),
            });
        }
        let parsed_tree = parse_newick_with_options(input, options)?;
        Ok(ParsedTreeInput {
            format: TreeInputFormat::Newick,
            tree_name: None,
            parsed_tree,
        })
    }
}

fn has_nexus_header(input: &str) -> bool {
    let input = input.strip_prefix('\u{feff}').unwrap_or(input).trim_start();
    input
        .get(..6)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("#nexus"))
}

fn parse_nexus_tree(
    input: &str,
    requested_tree_name: Option<&str>,
    options: NewickParseOptions,
) -> Result<ParsedTreeInput, NexusError> {
    let input = input.strip_prefix('\u{feff}').unwrap_or(input).trim_start();
    let header = input.get(..6).ok_or(NexusError::MissingHeader)?;
    if !header.eq_ignore_ascii_case("#nexus") {
        return Err(NexusError::MissingHeader);
    }

    let statements = split_statements(&input[6..])?;
    let mut saw_trees_block = false;
    let mut in_trees_block = false;
    let mut translations: Option<HashMap<String, String>> = None;
    let mut block_trees: Vec<NexusTree> = Vec::new();
    let mut trees: Vec<(NexusTree, HashMap<String, String>)> = Vec::new();

    for statement in statements {
        let Some((keyword, remainder)) = take_keyword(statement.text)? else {
            continue;
        };

        if !in_trees_block {
            if keyword.eq_ignore_ascii_case("begin") {
                let (block, tail) = take_atom(remainder, statement.position)?.ok_or(
                    NexusError::MissingBlockName {
                        position: statement.position,
                    },
                )?;
                ensure_ignored(tail, statement.position)?;
                if block.eq_ignore_ascii_case("trees") {
                    saw_trees_block = true;
                    in_trees_block = true;
                    translations = None;
                    block_trees.clear();
                }
            }
            continue;
        }

        if keyword.eq_ignore_ascii_case("end") || keyword.eq_ignore_ascii_case("endblock") {
            ensure_ignored(remainder, statement.position)?;
            let block_translations = translations.take().unwrap_or_default();
            for tree in block_trees.drain(..) {
                trees.push((tree, block_translations.clone()));
            }
            in_trees_block = false;
        } else if keyword.eq_ignore_ascii_case("translate") {
            if translations.is_some() {
                return Err(NexusError::DuplicateTranslate {
                    position: statement.position,
                });
            }
            translations = Some(parse_translate(remainder, statement.position)?);
        } else if keyword.eq_ignore_ascii_case("tree") {
            block_trees.push(parse_tree_statement(remainder, statement.position)?);
        } else if keyword.eq_ignore_ascii_case("utree") {
            return Err(NexusError::UnrootedTreeUnsupported {
                position: statement.position,
            });
        } else if keyword.eq_ignore_ascii_case("begin") {
            return Err(NexusError::NestedBlock {
                position: statement.position,
            });
        }
    }

    if in_trees_block {
        return Err(NexusError::UnterminatedTreesBlock);
    }
    if !saw_trees_block {
        return Err(NexusError::MissingTreesBlock);
    }
    if trees.is_empty() {
        return Err(NexusError::MissingTree);
    }
    let names = trees
        .iter()
        .map(|(tree, _)| tree.name.clone())
        .collect::<Vec<_>>();
    let mut seen_names = HashSet::new();
    if let Some(duplicate) = names.iter().find(|name| !seen_names.insert(name.as_str())) {
        return Err(NexusError::DuplicateTreeName {
            name: duplicate.clone(),
        });
    }

    let selected_index = match requested_tree_name {
        Some(requested) => trees
            .iter()
            .position(|(tree, _)| tree.name == requested)
            .ok_or_else(|| NexusError::TreeNotFound {
                requested: requested.to_string(),
                available: names.clone(),
            })?,
        None if trees.len() == 1 => 0,
        None => return Err(NexusError::MultipleTrees { names }),
    };
    let (tree, translations) = trees.swap_remove(selected_index);
    let mut newick = tree.newick;
    newick.push(';');
    let mut parsed_tree =
        parse_newick_with_options(&newick, options).map_err(|source| NexusError::InvalidTree {
            tree_name: tree.name.clone(),
            source,
        })?;
    apply_translations(&mut parsed_tree, &translations)?;

    Ok(ParsedTreeInput {
        format: TreeInputFormat::Nexus,
        tree_name: Some(tree.name),
        parsed_tree,
    })
}

#[derive(Clone, Debug)]
struct Statement<'a> {
    text: &'a str,
    position: usize,
}

fn split_statements(input: &str) -> Result<Vec<Statement<'_>>, NexusError> {
    let mut statements = Vec::new();
    let mut start = 0usize;
    let mut position = 0usize;
    let mut quote_start = None;
    let mut comment_starts = Vec::new();

    while position < input.len() {
        let ch = input[position..]
            .chars()
            .next()
            .expect("position is inside the input");
        if quote_start.is_some() {
            position += ch.len_utf8();
            if ch == '\'' {
                if input[position..].starts_with('\'') {
                    position += 1;
                } else {
                    quote_start = None;
                }
            }
            continue;
        }
        if !comment_starts.is_empty() {
            position += ch.len_utf8();
            match ch {
                '[' => comment_starts.push(position - 1),
                ']' => {
                    comment_starts.pop();
                }
                _ => {}
            }
            continue;
        }

        match ch {
            '\'' => {
                quote_start = Some(position);
                position += 1;
            }
            '[' => {
                comment_starts.push(position);
                position += 1;
            }
            ';' => {
                let text = input[start..position].trim();
                if !text.is_empty() {
                    let leading =
                        input[start..position].len() - input[start..position].trim_start().len();
                    statements.push(Statement {
                        text,
                        position: start + leading,
                    });
                }
                position += 1;
                start = position;
            }
            _ => position += ch.len_utf8(),
        }
    }

    if let Some(position) = quote_start {
        return Err(NexusError::UnterminatedQuote { position });
    }
    if let Some(position) = comment_starts.first() {
        return Err(NexusError::UnterminatedComment {
            position: *position,
        });
    }
    if !only_ignored(&input[start..])? {
        return Err(NexusError::MissingStatementTerminator { position: start });
    }

    Ok(statements)
}

fn take_keyword(input: &str) -> Result<Option<(&str, &str)>, NexusError> {
    let (input, _) = skip_ignored(input, 0)?;
    if input.is_empty() {
        return Ok(None);
    }
    let end = input
        .find(|ch: char| ch.is_whitespace() || ch == '[')
        .unwrap_or(input.len());
    Ok(Some((&input[..end], &input[end..])))
}

#[derive(Clone, Debug)]
struct NexusTree {
    name: String,
    newick: String,
}

fn parse_tree_statement(input: &str, position: usize) -> Result<NexusTree, NexusError> {
    let (left, right) = split_once_outside_ignored(input, '=', position)?
        .ok_or(NexusError::MissingTreeEquals { position })?;
    let (left, _) = skip_ignored(left, position)?;
    let left = left.strip_prefix('*').unwrap_or(left);
    let (name, tail) =
        take_atom(left, position)?.ok_or(NexusError::MissingTreeName { position })?;
    if name.is_empty() {
        return Err(NexusError::MissingTreeName { position });
    }
    ensure_ignored(tail, position)?;
    let (newick, _) = skip_ignored(right, position + left.len() + 1)?;
    if newick.is_empty() {
        return Err(NexusError::MissingTreeExpression {
            tree_name: name,
            position,
        });
    }

    Ok(NexusTree {
        name,
        newick: newick.to_string(),
    })
}

fn parse_translate(input: &str, position: usize) -> Result<HashMap<String, String>, NexusError> {
    let entries = split_outside_ignored(input, ',', position)?;
    let mut translations = HashMap::new();
    let mut labels = HashSet::new();

    for entry in entries {
        let (alias, remainder) =
            take_atom(entry, position)?.ok_or(NexusError::InvalidTranslateEntry { position })?;
        let (label, tail) = take_atom(remainder, position)?
            .ok_or(NexusError::InvalidTranslateEntry { position })?;
        ensure_ignored(tail, position)?;
        if translations.contains_key(&alias) {
            return Err(NexusError::DuplicateTranslateAlias { alias });
        }
        if !labels.insert(label.clone()) {
            return Err(NexusError::DuplicateTranslatedLabel { label });
        }
        translations.insert(alias, label);
    }

    if translations.is_empty() {
        return Err(NexusError::EmptyTranslate { position });
    }
    Ok(translations)
}

fn apply_translations(
    parsed_tree: &mut ParsedNewickTree,
    translations: &HashMap<String, String>,
) -> Result<(), NexusError> {
    if translations.is_empty() {
        return Ok(());
    }
    let mut translated_labels = HashSet::new();
    for tip in &mut parsed_tree.tip_labels {
        let translated =
            translations
                .get(&tip.label)
                .ok_or_else(|| NexusError::MissingTranslation {
                    alias: tip.label.clone(),
                })?;
        if !translated_labels.insert(translated.as_str()) {
            return Err(NexusError::DuplicateTranslatedLabel {
                label: translated.clone(),
            });
        }
        tip.label.clone_from(translated);
    }
    Ok(())
}

fn split_outside_ignored(
    input: &str,
    separator: char,
    base_position: usize,
) -> Result<Vec<&str>, NexusError> {
    let mut parts = Vec::new();
    let mut start = 0usize;
    let mut position = 0usize;
    let mut quoted = false;
    let mut comment_depth = 0usize;

    while position < input.len() {
        let ch = input[position..]
            .chars()
            .next()
            .expect("position is inside the input");
        if quoted {
            position += ch.len_utf8();
            if ch == '\'' {
                if input[position..].starts_with('\'') {
                    position += 1;
                } else {
                    quoted = false;
                }
            }
            continue;
        }
        if comment_depth > 0 {
            position += ch.len_utf8();
            match ch {
                '[' => comment_depth += 1,
                ']' => comment_depth -= 1,
                _ => {}
            }
            continue;
        }
        match ch {
            '\'' => {
                quoted = true;
                position += 1;
            }
            '[' => {
                comment_depth = 1;
                position += 1;
            }
            ch if ch == separator => {
                parts.push(input[start..position].trim());
                position += ch.len_utf8();
                start = position;
            }
            _ => position += ch.len_utf8(),
        }
    }

    if quoted {
        return Err(NexusError::UnterminatedQuote {
            position: base_position,
        });
    }
    if comment_depth > 0 {
        return Err(NexusError::UnterminatedComment {
            position: base_position,
        });
    }
    parts.push(input[start..].trim());
    Ok(parts)
}

fn split_once_outside_ignored(
    input: &str,
    separator: char,
    base_position: usize,
) -> Result<Option<(&str, &str)>, NexusError> {
    let parts = split_outside_ignored(input, separator, base_position)?;
    if parts.len() == 1 {
        return Ok(None);
    }
    if parts.len() > 2 {
        return Err(NexusError::UnexpectedSeparator {
            separator,
            position: base_position,
        });
    }
    Ok(Some((parts[0], parts[1])))
}

fn take_atom(input: &str, base_position: usize) -> Result<Option<(String, &str)>, NexusError> {
    let (input, skipped) = skip_ignored(input, base_position)?;
    if input.is_empty() {
        return Ok(None);
    }
    if input.starts_with('\'') {
        let mut value = String::new();
        let mut position = 1usize;
        while position < input.len() {
            let ch = input[position..]
                .chars()
                .next()
                .expect("position is inside the input");
            position += ch.len_utf8();
            if ch != '\'' {
                value.push(ch);
            } else if input[position..].starts_with('\'') {
                position += 1;
                value.push('\'');
            } else {
                return Ok(Some((value, &input[position..])));
            }
        }
        return Err(NexusError::UnterminatedQuote {
            position: base_position + skipped,
        });
    }

    let end = input
        .find(|ch: char| ch.is_whitespace() || matches!(ch, '[' | ']' | ',' | '='))
        .unwrap_or(input.len());
    if end == 0 {
        return Err(NexusError::InvalidAtom {
            position: base_position + skipped,
        });
    }
    Ok(Some((input[..end].to_string(), &input[end..])))
}

fn ensure_ignored(input: &str, base_position: usize) -> Result<(), NexusError> {
    if only_ignored(input)? {
        Ok(())
    } else {
        Err(NexusError::UnexpectedStatementText {
            position: base_position,
            text: input.trim().to_string(),
        })
    }
}

fn only_ignored(input: &str) -> Result<bool, NexusError> {
    let (remaining, _) = skip_ignored(input, 0)?;
    Ok(remaining.is_empty())
}

fn skip_ignored(mut input: &str, base_position: usize) -> Result<(&str, usize), NexusError> {
    let original_len = input.len();
    loop {
        input = input.trim_start();
        if !input.starts_with('[') {
            return Ok((input, original_len - input.len()));
        }
        let mut depth = 0usize;
        let mut end = None;
        for (offset, ch) in input.char_indices() {
            match ch {
                '[' => depth += 1,
                ']' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(offset + 1);
                        break;
                    }
                }
                _ => {}
            }
        }
        let end = end.ok_or(NexusError::UnterminatedComment {
            position: base_position + original_len - input.len(),
        })?;
        input = &input[end..];
    }
}

#[derive(Debug, PartialEq)]
pub enum TreeInputError {
    Newick(NewickError),
    Nexus(NexusError),
    TreeNameForNewick { tree_name: String },
}

impl fmt::Display for TreeInputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Newick(error) => write!(f, "invalid Newick input: {error}"),
            Self::Nexus(error) => write!(f, "invalid NEXUS input: {error}"),
            Self::TreeNameForNewick { tree_name } => write!(
                f,
                "tree name {tree_name:?} was provided for plain Newick input; tree-name selection is only valid for NEXUS"
            ),
        }
    }
}

impl Error for TreeInputError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Newick(error) => Some(error),
            Self::Nexus(error) => Some(error),
            Self::TreeNameForNewick { .. } => None,
        }
    }
}

impl From<NewickError> for TreeInputError {
    fn from(value: NewickError) -> Self {
        Self::Newick(value)
    }
}

#[derive(Debug, PartialEq)]
pub enum NexusError {
    MissingHeader,
    UnterminatedQuote {
        position: usize,
    },
    UnterminatedComment {
        position: usize,
    },
    MissingStatementTerminator {
        position: usize,
    },
    MissingBlockName {
        position: usize,
    },
    MissingTreesBlock,
    UnterminatedTreesBlock,
    NestedBlock {
        position: usize,
    },
    MissingTree,
    MultipleTrees {
        names: Vec<String>,
    },
    DuplicateTreeName {
        name: String,
    },
    TreeNotFound {
        requested: String,
        available: Vec<String>,
    },
    MissingTreeEquals {
        position: usize,
    },
    MissingTreeName {
        position: usize,
    },
    MissingTreeExpression {
        tree_name: String,
        position: usize,
    },
    UnrootedTreeUnsupported {
        position: usize,
    },
    DuplicateTranslate {
        position: usize,
    },
    EmptyTranslate {
        position: usize,
    },
    InvalidTranslateEntry {
        position: usize,
    },
    DuplicateTranslateAlias {
        alias: String,
    },
    DuplicateTranslatedLabel {
        label: String,
    },
    MissingTranslation {
        alias: String,
    },
    InvalidAtom {
        position: usize,
    },
    UnexpectedSeparator {
        separator: char,
        position: usize,
    },
    UnexpectedStatementText {
        position: usize,
        text: String,
    },
    InvalidTree {
        tree_name: String,
        source: NewickError,
    },
}

impl fmt::Display for NexusError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingHeader => write!(f, "missing #NEXUS header"),
            Self::UnterminatedQuote { position } => {
                write!(f, "unterminated quoted token at byte position {position}")
            }
            Self::UnterminatedComment { position } => {
                write!(f, "unterminated comment at byte position {position}")
            }
            Self::MissingStatementTerminator { position } => write!(
                f,
                "NEXUS statement at byte position {position} is missing its semicolon"
            ),
            Self::MissingBlockName { position } => {
                write!(
                    f,
                    "BEGIN statement at byte position {position} has no block name"
                )
            }
            Self::MissingTreesBlock => write!(f, "NEXUS input has no BEGIN TREES block"),
            Self::UnterminatedTreesBlock => {
                write!(f, "BEGIN TREES block has no terminating END statement")
            }
            Self::NestedBlock { position } => write!(
                f,
                "nested BEGIN statement inside TREES block at byte position {position}"
            ),
            Self::MissingTree => write!(f, "NEXUS TREES block contains no TREE statement"),
            Self::MultipleTrees { names } => write!(
                f,
                "NEXUS input contains multiple trees ({}); select one explicitly by name",
                names.join(", ")
            ),
            Self::DuplicateTreeName { name } => {
                write!(f, "NEXUS input contains duplicate TREE name {name:?}")
            }
            Self::TreeNotFound {
                requested,
                available,
            } => write!(
                f,
                "NEXUS tree {requested:?} was not found; available trees: {}",
                available.join(", ")
            ),
            Self::MissingTreeEquals { position } => write!(
                f,
                "TREE statement at byte position {position} is missing '='"
            ),
            Self::MissingTreeName { position } => {
                write!(f, "TREE statement at byte position {position} has no name")
            }
            Self::MissingTreeExpression {
                tree_name,
                position,
            } => write!(
                f,
                "TREE {tree_name:?} at byte position {position} has no Newick expression"
            ),
            Self::UnrootedTreeUnsupported { position } => write!(
                f,
                "UTREE statement at byte position {position} is unsupported; the likelihood engine requires a rooted tree"
            ),
            Self::DuplicateTranslate { position } => write!(
                f,
                "TREES block has more than one TRANSLATE statement (byte position {position})"
            ),
            Self::EmptyTranslate { position } => write!(
                f,
                "TRANSLATE statement at byte position {position} has no entries"
            ),
            Self::InvalidTranslateEntry { position } => write!(
                f,
                "TRANSLATE entry at byte position {position} must contain an alias and one taxon label"
            ),
            Self::DuplicateTranslateAlias { alias } => {
                write!(f, "duplicate TRANSLATE alias {alias:?}")
            }
            Self::DuplicateTranslatedLabel { label } => {
                write!(f, "duplicate translated tip label {label:?}")
            }
            Self::MissingTranslation { alias } => write!(
                f,
                "tree tip alias {alias:?} is absent from the TRANSLATE statement"
            ),
            Self::InvalidAtom { position } => {
                write!(f, "invalid NEXUS token at byte position {position}")
            }
            Self::UnexpectedSeparator {
                separator,
                position,
            } => write!(
                f,
                "unexpected additional {separator:?} at byte position {position}"
            ),
            Self::UnexpectedStatementText { position, text } => write!(
                f,
                "unexpected text {text:?} in NEXUS statement at byte position {position}"
            ),
            Self::InvalidTree { tree_name, source } => {
                write!(f, "failed to parse TREE {tree_name:?} as Newick: {source}")
            }
        }
    }
}

impl Error for NexusError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidTree { source, .. } => Some(source),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_plain_newick_on_the_existing_parser_path() {
        let parsed = parse_tree_input("(A:1,B:1);").unwrap();

        assert_eq!(parsed.format, TreeInputFormat::Newick);
        assert_eq!(parsed.tree_name, None);
        assert_eq!(parsed.parsed_tree.tip_node("A"), Some(0));
    }

    #[test]
    fn parses_case_insensitive_single_tree_nexus() {
        let parsed = parse_tree_input(
            "\u{feff}  #NeXuS\nBEGIN TAXA; DIMENSIONS NTAX=2; END;\n\
             begin trees; tree * analysis = [&R] (A:1,B:1); end;",
        )
        .unwrap();

        assert_eq!(parsed.format, TreeInputFormat::Nexus);
        assert_eq!(parsed.tree_name.as_deref(), Some("analysis"));
        assert_eq!(parsed.parsed_tree.tip_node("A"), Some(0));
        assert_eq!(parsed.parsed_tree.tip_node("B"), Some(1));
    }

    #[test]
    fn applies_translate_with_quoted_labels_comments_and_escaped_quotes() {
        let parsed = parse_tree_input(
            "#NEXUS\nBEGIN TREES;\n\
             TRANSLATE 1 'Homo sapiens', [nested[comment]] 2 'O''Brien fossil';\n\
             TREE 'dated tree' = [&R] (1:0.5,2:0.5);\nEND;",
        )
        .unwrap();

        assert_eq!(parsed.tree_name.as_deref(), Some("dated tree"));
        assert_eq!(parsed.parsed_tree.tip_node("Homo sapiens"), Some(0));
        assert_eq!(parsed.parsed_tree.tip_node("O'Brien fossil"), Some(1));
    }

    #[test]
    fn nexus_and_newick_build_identical_trees_after_translation() {
        let newick = parse_tree_input("(('Homo sapiens':1,chimp:1):1,gorilla:2);")
            .unwrap()
            .parsed_tree;
        let nexus = parse_tree_input(
            "#NEXUS\nBEGIN TREES;\nTRANSLATE 1 'Homo sapiens', 2 chimp, 3 gorilla;\n\
             TREE t1 = [&R] ((1:1,2:1):1,3:2);\nEND;",
        )
        .unwrap()
        .parsed_tree;

        assert_eq!(nexus, newick);
    }

    #[test]
    fn rejects_ambiguous_multiple_tree_input() {
        let error = parse_tree_input(
            "#NEXUS\nBEGIN TREES; TREE first=(A:1,B:1); TREE second=(A:2,B:2); END;",
        )
        .unwrap_err();

        assert_eq!(
            error,
            TreeInputError::Nexus(NexusError::MultipleTrees {
                names: vec!["first".to_string(), "second".to_string()]
            })
        );
    }

    #[test]
    fn explicitly_selects_one_named_tree_from_multi_tree_nexus() {
        let input = "#NEXUS\nBEGIN TREES;\n\
                     TRANSLATE 1 A, 2 B;\n\
                     TREE short = [&R] (1:1,2:1);\n\
                     TREE 'long tree' = [&R] (1:2,2:2);\nEND;";
        let selected = parse_tree_input_named(input, "long tree").unwrap();

        assert_eq!(selected.tree_name.as_deref(), Some("long tree"));
        assert!(
            selected
                .parsed_tree
                .tree
                .edges()
                .iter()
                .all(|edge| edge.length == 2.0)
        );
        assert_eq!(
            parse_tree_input_named(input, "missing").unwrap_err(),
            TreeInputError::Nexus(NexusError::TreeNotFound {
                requested: "missing".to_string(),
                available: vec!["short".to_string(), "long tree".to_string()]
            })
        );
    }

    #[test]
    fn rejects_tree_name_for_newick_and_duplicate_nexus_tree_names() {
        assert_eq!(
            parse_tree_input_named("(A:1,B:1);", "t1").unwrap_err(),
            TreeInputError::TreeNameForNewick {
                tree_name: "t1".to_string()
            }
        );
        assert_eq!(
            parse_tree_input_named(
                "#NEXUS\nBEGIN TREES; TREE same=(A:1,B:1); TREE same=(A:2,B:2); END;",
                "same"
            )
            .unwrap_err(),
            TreeInputError::Nexus(NexusError::DuplicateTreeName {
                name: "same".to_string()
            })
        );
        assert_eq!(
            parse_tree_input("#NEXUS\nBEGIN TREES; TREE ''=(A:1,B:1); END;").unwrap_err(),
            TreeInputError::Nexus(NexusError::MissingTreeName { position: 14 })
        );
    }

    #[test]
    fn rejects_missing_and_duplicate_translation_entries() {
        let missing =
            parse_tree_input("#NEXUS\nBEGIN TREES; TRANSLATE 1 A; TREE t=(1:1,2:1); END;")
                .unwrap_err();
        assert_eq!(
            missing,
            TreeInputError::Nexus(NexusError::MissingTranslation {
                alias: "2".to_string()
            })
        );

        let duplicate =
            parse_tree_input("#NEXUS\nBEGIN TREES; TRANSLATE 1 A, 2 A; TREE t=(1:1,2:1); END;")
                .unwrap_err();
        assert_eq!(
            duplicate,
            TreeInputError::Nexus(NexusError::DuplicateTranslatedLabel {
                label: "A".to_string()
            })
        );
    }

    #[test]
    fn rejects_unrooted_and_unterminated_tree_blocks() {
        assert!(matches!(
            parse_tree_input("#NEXUS\nBEGIN TREES; UTREE t=(A:1,B:1); END;"),
            Err(TreeInputError::Nexus(
                NexusError::UnrootedTreeUnsupported { .. }
            ))
        ));
        assert_eq!(
            parse_tree_input("#NEXUS\nBEGIN TREES; TREE t=(A:1,B:1);").unwrap_err(),
            TreeInputError::Nexus(NexusError::UnterminatedTreesBlock)
        );
    }
}
