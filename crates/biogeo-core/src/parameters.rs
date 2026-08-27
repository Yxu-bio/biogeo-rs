use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fmt;
use std::str::FromStr;

pub const BIOGEOBEARS_PARAMETER_NAMES: [&str; 23] = [
    "d", "e", "a", "b", "x", "n", "w", "u", "j", "ysv", "ys", "y", "s", "v", "mx01", "mx01j",
    "mx01y", "mx01s", "mx01v", "mx01r", "mf", "dp", "fdp",
];

pub const PARAMETER_TABLE_FORMAT_VERSION: &str = "biogeo-parameter-table-v1";
const PARAMETER_TABLE_HEADER: [&str; 7] = [
    "name",
    "mode",
    "value",
    "min",
    "max",
    "transform",
    "expression",
];

const BGB_MIN_ANAGENESIS: f64 = 1e-12;
const BGB_MIN_CLADOGENESIS: f64 = 1e-5;
const BGB_MAX_RATE: f64 = 5.0;
const BGB_MIN_MAXENT: f64 = 0.0001;
const BGB_MAX_MAXENT: f64 = 0.9999;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParameterBounds {
    pub min: f64,
    pub max: f64,
}

impl ParameterBounds {
    pub fn new(min: f64, max: f64) -> Result<Self, ParameterError> {
        if !min.is_finite() || !max.is_finite() || min > max {
            return Err(ParameterError::InvalidBounds { min, max });
        }
        Ok(Self { min, max })
    }

    pub fn contains(self, value: f64) -> bool {
        value >= self.min && value <= self.max
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ParameterTransform {
    #[default]
    Linear,
    Log,
    Logit,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ParameterMode {
    Fixed { value: f64 },
    Free { initial: f64 },
    Derived { expression: ParameterExpression },
}

#[derive(Clone, Debug, PartialEq)]
pub struct ParameterSpec {
    name: String,
    bounds: ParameterBounds,
    mode: ParameterMode,
    transform: ParameterTransform,
}

impl ParameterSpec {
    pub fn fixed(
        name: impl Into<String>,
        value: f64,
        bounds: ParameterBounds,
    ) -> Result<Self, ParameterError> {
        let name = validate_parameter_name(name.into())?;
        if !value.is_finite() {
            return Err(ParameterError::NonFiniteDeclaredValue {
                parameter: name,
                role: "fixed",
                value,
            });
        }
        Ok(Self {
            name,
            bounds,
            mode: ParameterMode::Fixed { value },
            transform: ParameterTransform::Linear,
        })
    }

    pub fn free(
        name: impl Into<String>,
        initial: f64,
        bounds: ParameterBounds,
    ) -> Result<Self, ParameterError> {
        let name = validate_parameter_name(name.into())?;
        if !initial.is_finite() {
            return Err(ParameterError::NonFiniteDeclaredValue {
                parameter: name,
                role: "free initial",
                value: initial,
            });
        }
        if !bounds.contains(initial) {
            return Err(ParameterError::InitialValueOutOfBounds {
                parameter: name,
                value: initial,
                bounds,
            });
        }
        if bounds.min == bounds.max {
            return Err(ParameterError::DegenerateFreeBounds {
                parameter: name,
                bounds,
            });
        }
        Ok(Self {
            name,
            bounds,
            mode: ParameterMode::Free { initial },
            transform: ParameterTransform::Linear,
        })
    }

    pub fn derived(
        name: impl Into<String>,
        expression: ParameterExpression,
        bounds: ParameterBounds,
    ) -> Result<Self, ParameterError> {
        let name = validate_parameter_name(name.into())?;
        Ok(Self {
            name,
            bounds,
            mode: ParameterMode::Derived { expression },
            transform: ParameterTransform::Linear,
        })
    }

    pub fn derived_from_str(
        name: impl Into<String>,
        expression: &str,
        bounds: ParameterBounds,
    ) -> Result<Self, ParameterError> {
        let name = name.into();
        let parsed = ParameterExpression::from_str(expression).map_err(|source| {
            ParameterError::ExpressionParse {
                parameter: name.clone(),
                source,
            }
        })?;
        Self::derived(name, parsed, bounds)
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn bounds(&self) -> ParameterBounds {
        self.bounds
    }

    pub fn mode(&self) -> &ParameterMode {
        &self.mode
    }

    pub fn transform(&self) -> ParameterTransform {
        self.transform
    }

    pub fn with_transform(mut self, transform: ParameterTransform) -> Result<Self, ParameterError> {
        match transform {
            ParameterTransform::Linear => {}
            ParameterTransform::Log if self.bounds.min <= 0.0 => {
                return Err(ParameterError::InvalidTransformBounds {
                    parameter: self.name,
                    transform,
                    bounds: self.bounds,
                });
            }
            ParameterTransform::Log => {}
            ParameterTransform::Logit if self.bounds.min >= self.bounds.max => {
                return Err(ParameterError::InvalidTransformBounds {
                    parameter: self.name,
                    transform,
                    bounds: self.bounds,
                });
            }
            ParameterTransform::Logit => {
                if let ParameterMode::Free { initial } = self.mode
                    && (initial <= self.bounds.min || initial >= self.bounds.max)
                {
                    return Err(ParameterError::InitialValueNotStrictlyInsideBounds {
                        parameter: self.name,
                        value: initial,
                        bounds: self.bounds,
                        transform,
                    });
                }
            }
        }
        self.transform = transform;
        Ok(self)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ParameterExpression {
    Constant(f64),
    Reference(String),
    Negate(Box<Self>),
    Add(Box<Self>, Box<Self>),
    Subtract(Box<Self>, Box<Self>),
    Multiply(Box<Self>, Box<Self>),
    Divide(Box<Self>, Box<Self>),
}

impl ParameterExpression {
    fn collect_references<'a>(&'a self, references: &mut Vec<&'a str>) {
        match self {
            Self::Constant(_) => {}
            Self::Reference(name) => references.push(name),
            Self::Negate(value) => value.collect_references(references),
            Self::Add(left, right)
            | Self::Subtract(left, right)
            | Self::Multiply(left, right)
            | Self::Divide(left, right) => {
                left.collect_references(references);
                right.collect_references(references);
            }
        }
    }
}

impl fmt::Display for ParameterExpression {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Constant(value) => write!(f, "{value}"),
            Self::Reference(name) => f.write_str(name),
            Self::Negate(value) => write!(f, "-({value})"),
            Self::Add(left, right) => write!(f, "({left})+({right})"),
            Self::Subtract(left, right) => write!(f, "({left})-({right})"),
            Self::Multiply(left, right) => write!(f, "({left})*({right})"),
            Self::Divide(left, right) => write!(f, "({left})/({right})"),
        }
    }
}

impl FromStr for ParameterExpression {
    type Err = ParameterExpressionParseError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        ExpressionParser::new(input).parse()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParameterExpressionParseError {
    position: usize,
    message: String,
}

impl ParameterExpressionParseError {
    fn new(position: usize, message: impl Into<String>) -> Self {
        Self {
            position,
            message: message.into(),
        }
    }

    pub fn position(&self) -> usize {
        self.position
    }
}

impl fmt::Display for ParameterExpressionParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "parameter expression error at byte {}: {}",
            self.position, self.message
        )
    }
}

impl Error for ParameterExpressionParseError {}

struct ExpressionParser<'a> {
    input: &'a str,
    position: usize,
}

impl<'a> ExpressionParser<'a> {
    fn new(input: &'a str) -> Self {
        Self { input, position: 0 }
    }

    fn parse(mut self) -> Result<ParameterExpression, ParameterExpressionParseError> {
        self.skip_whitespace();
        if self.position == self.input.len() {
            return Err(ParameterExpressionParseError::new(0, "expression is empty"));
        }
        let expression = self.parse_sum()?;
        self.skip_whitespace();
        if self.position != self.input.len() {
            return Err(ParameterExpressionParseError::new(
                self.position,
                format!(
                    "unexpected character '{}'",
                    self.input[self.position..].chars().next().unwrap_or('\0')
                ),
            ));
        }
        Ok(expression)
    }

    fn parse_sum(&mut self) -> Result<ParameterExpression, ParameterExpressionParseError> {
        let mut expression = self.parse_product()?;
        loop {
            self.skip_whitespace();
            if self.consume_byte(b'+') {
                expression =
                    ParameterExpression::Add(Box::new(expression), Box::new(self.parse_product()?));
            } else if self.consume_byte(b'-') {
                expression = ParameterExpression::Subtract(
                    Box::new(expression),
                    Box::new(self.parse_product()?),
                );
            } else {
                return Ok(expression);
            }
        }
    }

    fn parse_product(&mut self) -> Result<ParameterExpression, ParameterExpressionParseError> {
        let mut expression = self.parse_unary()?;
        loop {
            self.skip_whitespace();
            if self.consume_byte(b'*') {
                expression = ParameterExpression::Multiply(
                    Box::new(expression),
                    Box::new(self.parse_unary()?),
                );
            } else if self.consume_byte(b'/') {
                expression = ParameterExpression::Divide(
                    Box::new(expression),
                    Box::new(self.parse_unary()?),
                );
            } else {
                return Ok(expression);
            }
        }
    }

    fn parse_unary(&mut self) -> Result<ParameterExpression, ParameterExpressionParseError> {
        self.skip_whitespace();
        if self.consume_byte(b'+') {
            self.parse_unary()
        } else if self.consume_byte(b'-') {
            Ok(ParameterExpression::Negate(Box::new(self.parse_unary()?)))
        } else {
            self.parse_primary()
        }
    }

    fn parse_primary(&mut self) -> Result<ParameterExpression, ParameterExpressionParseError> {
        self.skip_whitespace();
        let Some(byte) = self.peek_byte() else {
            return Err(ParameterExpressionParseError::new(
                self.position,
                "expected a number, parameter name, or '('",
            ));
        };

        if byte == b'(' {
            self.position += 1;
            let expression = self.parse_sum()?;
            self.skip_whitespace();
            if !self.consume_byte(b')') {
                return Err(ParameterExpressionParseError::new(
                    self.position,
                    "expected ')'",
                ));
            }
            return Ok(expression);
        }

        if byte.is_ascii_alphabetic() || byte == b'_' {
            return Ok(ParameterExpression::Reference(self.parse_identifier()));
        }

        if byte.is_ascii_digit() || byte == b'.' {
            return self.parse_number().map(ParameterExpression::Constant);
        }

        Err(ParameterExpressionParseError::new(
            self.position,
            format!("unexpected character '{}'", char::from(byte)),
        ))
    }

    fn parse_identifier(&mut self) -> String {
        let start = self.position;
        self.position += 1;
        while self
            .peek_byte()
            .is_some_and(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        {
            self.position += 1;
        }
        self.input[start..self.position].to_owned()
    }

    fn parse_number(&mut self) -> Result<f64, ParameterExpressionParseError> {
        let start = self.position;
        let mut digit_count = 0;

        while self.peek_byte().is_some_and(|byte| byte.is_ascii_digit()) {
            digit_count += 1;
            self.position += 1;
        }
        if self.consume_byte(b'.') {
            while self.peek_byte().is_some_and(|byte| byte.is_ascii_digit()) {
                digit_count += 1;
                self.position += 1;
            }
        }
        if digit_count == 0 {
            return Err(ParameterExpressionParseError::new(
                start,
                "invalid numeric literal",
            ));
        }

        if self
            .peek_byte()
            .is_some_and(|byte| byte == b'e' || byte == b'E')
        {
            self.position += 1;
            if self
                .peek_byte()
                .is_some_and(|byte| byte == b'+' || byte == b'-')
            {
                self.position += 1;
            }
            let exponent_start = self.position;
            while self.peek_byte().is_some_and(|byte| byte.is_ascii_digit()) {
                self.position += 1;
            }
            if self.position == exponent_start {
                return Err(ParameterExpressionParseError::new(
                    exponent_start,
                    "numeric exponent has no digits",
                ));
            }
        }

        let literal = &self.input[start..self.position];
        let value = literal.parse::<f64>().map_err(|_| {
            ParameterExpressionParseError::new(start, format!("invalid number '{literal}'"))
        })?;
        if !value.is_finite() {
            return Err(ParameterExpressionParseError::new(
                start,
                format!("number '{literal}' is not finite"),
            ));
        }
        Ok(value)
    }

    fn skip_whitespace(&mut self) {
        while self
            .peek_byte()
            .is_some_and(|byte| byte.is_ascii_whitespace())
        {
            self.position += 1;
        }
    }

    fn peek_byte(&self) -> Option<u8> {
        self.input.as_bytes().get(self.position).copied()
    }

    fn consume_byte(&mut self, expected: u8) -> bool {
        if self.peek_byte() == Some(expected) {
            self.position += 1;
            true
        } else {
            false
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ParameterTable {
    specs: Vec<ParameterSpec>,
    index_by_name: HashMap<String, usize>,
}

impl ParameterTable {
    pub fn new(specs: Vec<ParameterSpec>) -> Result<Self, ParameterError> {
        let mut index_by_name = HashMap::with_capacity(specs.len());
        for (index, spec) in specs.iter().enumerate() {
            if index_by_name.insert(spec.name.clone(), index).is_some() {
                return Err(ParameterError::DuplicateParameter {
                    parameter: spec.name.clone(),
                });
            }
        }

        let table = Self {
            specs,
            index_by_name,
        };
        table.validate_dependencies()?;
        table.resolve_initial()?;
        Ok(table)
    }

    pub fn specs(&self) -> &[ParameterSpec] {
        &self.specs
    }

    pub fn to_versioned_tsv(&self) -> String {
        let mut output = String::new();
        output.push_str(PARAMETER_TABLE_FORMAT_VERSION);
        output.push('\n');
        output.push_str(&PARAMETER_TABLE_HEADER.join("\t"));
        output.push('\n');
        for spec in &self.specs {
            let (mode, value, expression) = match &spec.mode {
                ParameterMode::Fixed { value } => ("fixed", value.to_string(), String::new()),
                ParameterMode::Free { initial } => ("free", initial.to_string(), String::new()),
                ParameterMode::Derived { expression } => {
                    ("derived", String::new(), expression.to_string())
                }
            };
            let transform = match spec.transform {
                ParameterTransform::Linear => "linear",
                ParameterTransform::Log => "log",
                ParameterTransform::Logit => "logit",
            };
            output.push_str(&spec.name);
            output.push('\t');
            output.push_str(mode);
            output.push('\t');
            output.push_str(&value);
            output.push('\t');
            output.push_str(&spec.bounds.min.to_string());
            output.push('\t');
            output.push_str(&spec.bounds.max.to_string());
            output.push('\t');
            output.push_str(transform);
            output.push('\t');
            output.push_str(&expression);
            output.push('\n');
        }
        output
    }

    pub fn spec(&self, name: &str) -> Option<&ParameterSpec> {
        self.index_by_name
            .get(name)
            .map(|index| &self.specs[*index])
    }

    pub fn free_parameter_names(&self) -> Vec<&str> {
        self.specs
            .iter()
            .filter_map(|spec| match spec.mode {
                ParameterMode::Free { .. } => Some(spec.name.as_str()),
                _ => None,
            })
            .collect()
    }

    pub fn free_parameter_specs(&self) -> Vec<&ParameterSpec> {
        self.specs
            .iter()
            .filter(|spec| matches!(spec.mode, ParameterMode::Free { .. }))
            .collect()
    }

    pub fn free_parameters_affecting<'a>(
        &'a self,
        targets: &[&str],
    ) -> Result<Vec<&'a str>, ParameterError> {
        let mut affecting = HashSet::new();
        for target in targets {
            let index = self.parameter_index(target)?;
            self.collect_dependencies(index, &mut affecting);
        }
        Ok(self
            .specs
            .iter()
            .filter_map(|spec| {
                (matches!(spec.mode, ParameterMode::Free { .. })
                    && affecting.contains(spec.name.as_str()))
                .then_some(spec.name.as_str())
            })
            .collect())
    }

    pub fn initial_free_values(&self) -> Vec<f64> {
        self.specs
            .iter()
            .filter_map(|spec| match spec.mode {
                ParameterMode::Free { initial } => Some(initial),
                _ => None,
            })
            .collect()
    }

    pub fn resolve_initial(&self) -> Result<ResolvedParameters, ParameterError> {
        self.resolve_free_values(&self.initial_free_values())
    }

    pub fn with_fixed(mut self, name: &str, value: f64) -> Result<Self, ParameterError> {
        let index = self.parameter_index(name)?;
        let bounds = self.specs[index].bounds;
        let transform = self.specs[index].transform;
        self.specs[index] = ParameterSpec::fixed(name, value, bounds)?.with_transform(transform)?;
        Self::new(self.specs)
    }

    pub fn with_free(
        mut self,
        name: &str,
        initial: f64,
        bounds: ParameterBounds,
    ) -> Result<Self, ParameterError> {
        let index = self.parameter_index(name)?;
        let transform = self.specs[index].transform;
        self.specs[index] =
            ParameterSpec::free(name, initial, bounds)?.with_transform(transform)?;
        Self::new(self.specs)
    }

    pub fn with_derived(
        mut self,
        name: &str,
        expression: ParameterExpression,
    ) -> Result<Self, ParameterError> {
        let index = self.parameter_index(name)?;
        let bounds = self.specs[index].bounds;
        let transform = self.specs[index].transform;
        self.specs[index] =
            ParameterSpec::derived(name, expression, bounds)?.with_transform(transform)?;
        Self::new(self.specs)
    }

    pub fn with_derived_from_str(
        mut self,
        name: &str,
        expression: &str,
    ) -> Result<Self, ParameterError> {
        let index = self.parameter_index(name)?;
        let bounds = self.specs[index].bounds;
        let transform = self.specs[index].transform;
        self.specs[index] =
            ParameterSpec::derived_from_str(name, expression, bounds)?.with_transform(transform)?;
        Self::new(self.specs)
    }

    pub fn with_transform(
        mut self,
        name: &str,
        transform: ParameterTransform,
    ) -> Result<Self, ParameterError> {
        let index = self.parameter_index(name)?;
        self.specs[index] = self.specs[index].clone().with_transform(transform)?;
        Self::new(self.specs)
    }

    pub fn resolve_free_values(
        &self,
        free_values: &[f64],
    ) -> Result<ResolvedParameters, ParameterError> {
        let expected = self.free_parameter_names().len();
        if free_values.len() != expected {
            return Err(ParameterError::FreeValueCountMismatch {
                expected,
                actual: free_values.len(),
            });
        }

        let mut values = vec![None; self.specs.len()];
        let mut states = vec![ResolutionState::Unvisited; self.specs.len()];
        let mut free_index = 0;
        for (index, spec) in self.specs.iter().enumerate() {
            match spec.mode {
                ParameterMode::Fixed { value } => {
                    values[index] = Some(value);
                    states[index] = ResolutionState::Resolved;
                }
                ParameterMode::Free { .. } => {
                    let value = free_values[free_index];
                    free_index += 1;
                    if !value.is_finite() {
                        return Err(ParameterError::NonFiniteFreeValue {
                            parameter: spec.name.clone(),
                            value,
                        });
                    }
                    if !spec.bounds.contains(value) {
                        return Err(ParameterError::FreeValueOutOfBounds {
                            parameter: spec.name.clone(),
                            value,
                            bounds: spec.bounds,
                        });
                    }
                    values[index] = Some(value);
                    states[index] = ResolutionState::Resolved;
                }
                ParameterMode::Derived { .. } => {}
            }
        }

        for index in 0..self.specs.len() {
            self.resolve_index(index, &mut values, &mut states)?;
        }

        let entries = self
            .specs
            .iter()
            .zip(values)
            .map(|(spec, value)| {
                (
                    spec.name.clone(),
                    value.expect("all parameters are resolved before result construction"),
                )
            })
            .collect();
        Ok(ResolvedParameters::new(entries))
    }

    fn validate_dependencies(&self) -> Result<(), ParameterError> {
        for spec in &self.specs {
            if let ParameterMode::Derived { expression } = &spec.mode {
                let mut references = Vec::new();
                expression.collect_references(&mut references);
                for reference in references {
                    if !self.index_by_name.contains_key(reference) {
                        return Err(ParameterError::UnknownReference {
                            parameter: spec.name.clone(),
                            reference: reference.to_owned(),
                        });
                    }
                }
            }
        }

        let mut states = vec![DependencyState::Unvisited; self.specs.len()];
        let mut stack = Vec::new();
        for index in 0..self.specs.len() {
            self.visit_dependency(index, &mut states, &mut stack)?;
        }
        Ok(())
    }

    fn collect_dependencies<'a>(&'a self, index: usize, names: &mut HashSet<&'a str>) {
        let spec = &self.specs[index];
        if !names.insert(spec.name.as_str()) {
            return;
        }
        if let ParameterMode::Derived { expression } = &spec.mode {
            let mut references = Vec::new();
            expression.collect_references(&mut references);
            for reference in references {
                self.collect_dependencies(self.index_by_name[reference], names);
            }
        }
    }

    fn parameter_index(&self, name: &str) -> Result<usize, ParameterError> {
        self.index_by_name
            .get(name)
            .copied()
            .ok_or_else(|| ParameterError::UnknownParameter {
                parameter: name.to_owned(),
            })
    }

    fn visit_dependency(
        &self,
        index: usize,
        states: &mut [DependencyState],
        stack: &mut Vec<usize>,
    ) -> Result<(), ParameterError> {
        match states[index] {
            DependencyState::Resolved => return Ok(()),
            DependencyState::Visiting => {
                let cycle_start = stack
                    .iter()
                    .position(|candidate| *candidate == index)
                    .unwrap_or(0);
                let mut cycle = stack[cycle_start..]
                    .iter()
                    .map(|item| self.specs[*item].name.clone())
                    .collect::<Vec<_>>();
                cycle.push(self.specs[index].name.clone());
                return Err(ParameterError::CyclicDependency { cycle });
            }
            DependencyState::Unvisited => {}
        }

        states[index] = DependencyState::Visiting;
        stack.push(index);
        if let ParameterMode::Derived { expression } = &self.specs[index].mode {
            let mut references = Vec::new();
            expression.collect_references(&mut references);
            for reference in references {
                let reference_index = self.index_by_name[reference];
                self.visit_dependency(reference_index, states, stack)?;
            }
        }
        stack.pop();
        states[index] = DependencyState::Resolved;
        Ok(())
    }

    fn resolve_index(
        &self,
        index: usize,
        values: &mut [Option<f64>],
        states: &mut [ResolutionState],
    ) -> Result<f64, ParameterError> {
        if let Some(value) = values[index] {
            return Ok(value);
        }
        if states[index] == ResolutionState::Visiting {
            return Err(ParameterError::CyclicDependency {
                cycle: vec![self.specs[index].name.clone()],
            });
        }

        states[index] = ResolutionState::Visiting;
        let expression = match &self.specs[index].mode {
            ParameterMode::Derived { expression } => expression.clone(),
            _ => unreachable!("fixed and free parameters are initialized before resolution"),
        };
        let parameter = self.specs[index].name.clone();
        let value = self.evaluate_expression(&parameter, &expression, values, states)?;
        if !value.is_finite() {
            return Err(ParameterError::NonFiniteDerivedValue { parameter, value });
        }
        if !self.specs[index].bounds.contains(value) {
            return Err(ParameterError::DerivedValueOutOfBounds {
                parameter,
                value,
                bounds: self.specs[index].bounds,
            });
        }
        values[index] = Some(value);
        states[index] = ResolutionState::Resolved;
        Ok(value)
    }

    fn evaluate_expression(
        &self,
        parameter: &str,
        expression: &ParameterExpression,
        values: &mut [Option<f64>],
        states: &mut [ResolutionState],
    ) -> Result<f64, ParameterError> {
        let value = match expression {
            ParameterExpression::Constant(value) => *value,
            ParameterExpression::Reference(name) => {
                let index = self.index_by_name[name];
                self.resolve_index(index, values, states)?
            }
            ParameterExpression::Negate(value) => {
                -self.evaluate_expression(parameter, value, values, states)?
            }
            ParameterExpression::Add(left, right) => {
                self.evaluate_expression(parameter, left, values, states)?
                    + self.evaluate_expression(parameter, right, values, states)?
            }
            ParameterExpression::Subtract(left, right) => {
                self.evaluate_expression(parameter, left, values, states)?
                    - self.evaluate_expression(parameter, right, values, states)?
            }
            ParameterExpression::Multiply(left, right) => {
                self.evaluate_expression(parameter, left, values, states)?
                    * self.evaluate_expression(parameter, right, values, states)?
            }
            ParameterExpression::Divide(left, right) => {
                let numerator = self.evaluate_expression(parameter, left, values, states)?;
                let denominator = self.evaluate_expression(parameter, right, values, states)?;
                if denominator == 0.0 {
                    return Err(ParameterError::DivisionByZero {
                        parameter: parameter.to_owned(),
                    });
                }
                numerator / denominator
            }
        };

        if !value.is_finite() {
            return Err(ParameterError::NonFiniteDerivedValue {
                parameter: parameter.to_owned(),
                value,
            });
        }
        Ok(value)
    }
}

pub fn parse_parameter_table(input: &str) -> Result<ParameterTable, ParameterTableParseError> {
    let mut lines = input.lines().enumerate().filter_map(|(index, line)| {
        let trimmed = line.trim();
        (!trimmed.is_empty() && !trimmed.starts_with('#')).then_some((index + 1, line))
    });

    let Some((version_line, raw_version)) = lines.next() else {
        return Err(ParameterTableParseError::EmptyInput);
    };
    let version = raw_version.trim().trim_start_matches('\u{feff}');
    if version != PARAMETER_TABLE_FORMAT_VERSION {
        return Err(ParameterTableParseError::UnsupportedVersion {
            line: version_line,
            actual: version.to_owned(),
        });
    }

    let Some((header_line, raw_header)) = lines.next() else {
        return Err(ParameterTableParseError::MissingHeader);
    };
    let header = split_parameter_table_row(raw_header);
    if header.as_slice() != PARAMETER_TABLE_HEADER {
        return Err(ParameterTableParseError::InvalidHeader {
            line: header_line,
            actual: header.into_iter().map(str::to_owned).collect(),
        });
    }

    let mut specs = Vec::new();
    for (line, raw_row) in lines {
        let fields = split_parameter_table_row(raw_row);
        if fields.len() != PARAMETER_TABLE_HEADER.len() {
            return Err(ParameterTableParseError::WrongColumnCount {
                line,
                actual: fields.len(),
            });
        }
        let name = fields[0];
        let mode = fields[1];
        let value = fields[2];
        let min = parse_parameter_table_number(line, "min", fields[3])?;
        let max = parse_parameter_table_number(line, "max", fields[4])?;
        let bounds = ParameterBounds::new(min, max)
            .map_err(|source| ParameterTableParseError::Parameter { line, source })?;
        let transform = match fields[5] {
            "linear" => ParameterTransform::Linear,
            "log" => ParameterTransform::Log,
            "logit" => ParameterTransform::Logit,
            actual => {
                return Err(ParameterTableParseError::UnknownTransform {
                    line,
                    actual: actual.to_owned(),
                });
            }
        };
        let expression = fields[6];
        let spec = match mode {
            "fixed" | "free" => {
                if value.is_empty() {
                    return Err(ParameterTableParseError::MissingModeField {
                        line,
                        mode: mode.to_owned(),
                        field: "value",
                    });
                }
                if !expression.is_empty() {
                    return Err(ParameterTableParseError::UnexpectedModeField {
                        line,
                        mode: mode.to_owned(),
                        field: "expression",
                    });
                }
                let value = parse_parameter_table_number(line, "value", value)?;
                if mode == "fixed" {
                    ParameterSpec::fixed(name, value, bounds)
                } else {
                    ParameterSpec::free(name, value, bounds)
                }
            }
            "derived" => {
                if !value.is_empty() {
                    return Err(ParameterTableParseError::UnexpectedModeField {
                        line,
                        mode: mode.to_owned(),
                        field: "value",
                    });
                }
                if expression.is_empty() {
                    return Err(ParameterTableParseError::MissingModeField {
                        line,
                        mode: mode.to_owned(),
                        field: "expression",
                    });
                }
                ParameterSpec::derived_from_str(name, expression, bounds)
            }
            actual => {
                return Err(ParameterTableParseError::UnknownMode {
                    line,
                    actual: actual.to_owned(),
                });
            }
        }
        .and_then(|spec| spec.with_transform(transform))
        .map_err(|source| ParameterTableParseError::Parameter { line, source })?;
        specs.push(spec);
    }
    if specs.is_empty() {
        return Err(ParameterTableParseError::NoParameters);
    }
    ParameterTable::new(specs).map_err(|source| ParameterTableParseError::Table { source })
}

fn split_parameter_table_row(row: &str) -> Vec<&str> {
    row.trim_end_matches('\r')
        .split('\t')
        .map(str::trim)
        .collect()
}

fn parse_parameter_table_number(
    line: usize,
    field: &'static str,
    value: &str,
) -> Result<f64, ParameterTableParseError> {
    value
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite())
        .ok_or_else(|| ParameterTableParseError::InvalidNumber {
            line,
            field,
            value: value.to_owned(),
        })
}

#[derive(Clone, Debug, PartialEq)]
pub enum ParameterTableParseError {
    EmptyInput,
    UnsupportedVersion {
        line: usize,
        actual: String,
    },
    MissingHeader,
    InvalidHeader {
        line: usize,
        actual: Vec<String>,
    },
    WrongColumnCount {
        line: usize,
        actual: usize,
    },
    InvalidNumber {
        line: usize,
        field: &'static str,
        value: String,
    },
    UnknownMode {
        line: usize,
        actual: String,
    },
    UnknownTransform {
        line: usize,
        actual: String,
    },
    MissingModeField {
        line: usize,
        mode: String,
        field: &'static str,
    },
    UnexpectedModeField {
        line: usize,
        mode: String,
        field: &'static str,
    },
    Parameter {
        line: usize,
        source: ParameterError,
    },
    NoParameters,
    Table {
        source: ParameterError,
    },
}

impl fmt::Display for ParameterTableParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyInput => write!(f, "parameter table is empty"),
            Self::UnsupportedVersion { line, actual } => write!(
                f,
                "parameter table line {line} has unsupported format version {actual:?}; expected {PARAMETER_TABLE_FORMAT_VERSION:?}"
            ),
            Self::MissingHeader => write!(f, "parameter table is missing its column header"),
            Self::InvalidHeader { line, actual } => write!(
                f,
                "parameter table header on line {line} must be {:?}, got {actual:?}",
                PARAMETER_TABLE_HEADER
            ),
            Self::WrongColumnCount { line, actual } => write!(
                f,
                "parameter table row on line {line} has {actual} columns, expected {}",
                PARAMETER_TABLE_HEADER.len()
            ),
            Self::InvalidNumber { line, field, value } => write!(
                f,
                "parameter table {field} on line {line} is not a finite number: {value:?}"
            ),
            Self::UnknownMode { line, actual } => write!(
                f,
                "parameter table mode on line {line} must be fixed, free, or derived, got {actual:?}"
            ),
            Self::UnknownTransform { line, actual } => write!(
                f,
                "parameter table transform on line {line} must be linear, log, or logit, got {actual:?}"
            ),
            Self::MissingModeField { line, mode, field } => write!(
                f,
                "parameter table {mode} row on line {line} requires a non-empty {field} field"
            ),
            Self::UnexpectedModeField { line, mode, field } => write!(
                f,
                "parameter table {mode} row on line {line} requires an empty {field} field"
            ),
            Self::Parameter { line, source } => {
                write!(f, "invalid parameter table row on line {line}: {source}")
            }
            Self::NoParameters => write!(f, "parameter table contains no parameter rows"),
            Self::Table { source } => write!(f, "invalid parameter table: {source}"),
        }
    }
}

impl Error for ParameterTableParseError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Parameter { source, .. } | Self::Table { source } => Some(source),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DependencyState {
    Unvisited,
    Visiting,
    Resolved,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ResolutionState {
    Unvisited,
    Visiting,
    Resolved,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedParameters {
    entries: Vec<(String, f64)>,
    index_by_name: HashMap<String, usize>,
}

impl ResolvedParameters {
    fn new(entries: Vec<(String, f64)>) -> Self {
        let index_by_name = entries
            .iter()
            .enumerate()
            .map(|(index, (name, _))| (name.clone(), index))
            .collect();
        Self {
            entries,
            index_by_name,
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn get(&self, name: &str) -> Option<f64> {
        self.index_by_name
            .get(name)
            .map(|index| self.entries[*index].1)
    }

    pub fn require(&self, name: &str) -> Result<f64, ParameterError> {
        self.get(name)
            .ok_or_else(|| ParameterError::MissingParameter {
                parameter: name.to_owned(),
            })
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, f64)> {
        self.entries
            .iter()
            .map(|(name, value)| (name.as_str(), *value))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BioGeoBearsPreset {
    Dec,
    DecJ,
    DivaLike,
    DivaLikeJ,
    BayAreaLike,
    BayAreaLikeJ,
}

impl BioGeoBearsPreset {
    pub fn parameter_table(self) -> Result<ParameterTable, ParameterError> {
        let mut specs = biogeobears_default_specs()?;
        match self {
            Self::Dec => {}
            Self::DecJ => configure_dec_j(&mut specs)?,
            Self::DivaLike => configure_divalike(&mut specs, false)?,
            Self::DivaLikeJ => configure_divalike(&mut specs, true)?,
            Self::BayAreaLike => configure_bayarealike(&mut specs, false)?,
            Self::BayAreaLikeJ => configure_bayarealike(&mut specs, true)?,
        }
        ParameterTable::new(specs)
    }
}

pub fn biogeobears_default_parameter_table() -> Result<ParameterTable, ParameterError> {
    BioGeoBearsPreset::Dec.parameter_table()
}

fn biogeobears_default_specs() -> Result<Vec<ParameterSpec>, ParameterError> {
    let rate_bounds = ParameterBounds::new(BGB_MIN_ANAGENESIS, BGB_MAX_RATE - BGB_MIN_ANAGENESIS)?;
    let clado_j_bounds = ParameterBounds::new(BGB_MIN_CLADOGENESIS, 3.0 - BGB_MIN_CLADOGENESIS)?;
    let maxent_bounds = ParameterBounds::new(BGB_MIN_MAXENT, BGB_MAX_MAXENT)?;
    let probability_bounds = ParameterBounds::new(0.005, 0.995)?;

    let specs = vec![
        ParameterSpec::free("d", 0.01, rate_bounds)?,
        ParameterSpec::free("e", 0.01, rate_bounds)?,
        ParameterSpec::fixed("a", 0.0, rate_bounds)?,
        ParameterSpec::fixed(
            "b",
            1.0,
            ParameterBounds::new(BGB_MIN_ANAGENESIS, 1.0 - BGB_MIN_ANAGENESIS)?,
        )?,
        ParameterSpec::fixed("x", 0.0, ParameterBounds::new(-2.5, 2.5)?)?,
        ParameterSpec::fixed("n", 0.0, ParameterBounds::new(-10.0, 10.0)?)?,
        ParameterSpec::fixed("w", 1.0, ParameterBounds::new(-10.0, 10.0)?)?,
        ParameterSpec::fixed("u", 0.0, ParameterBounds::new(-10.0, 10.0)?)?,
        ParameterSpec::fixed("j", 0.0, clado_j_bounds)?,
        ParameterSpec::derived_from_str(
            "ysv",
            "3-j",
            ParameterBounds::new(BGB_MIN_CLADOGENESIS, 3.0)?,
        )?,
        ParameterSpec::derived_from_str(
            "ys",
            "ysv*2/3",
            ParameterBounds::new(BGB_MIN_CLADOGENESIS, 2.0)?,
        )?,
        ParameterSpec::derived_from_str(
            "y",
            "ysv*1/3",
            ParameterBounds::new(BGB_MIN_CLADOGENESIS, 1.0)?,
        )?,
        ParameterSpec::derived_from_str(
            "s",
            "ysv*1/3",
            ParameterBounds::new(BGB_MIN_CLADOGENESIS, 1.0)?,
        )?,
        ParameterSpec::derived_from_str(
            "v",
            "ysv*1/3",
            ParameterBounds::new(BGB_MIN_CLADOGENESIS, 1.0)?,
        )?,
        ParameterSpec::fixed("mx01", BGB_MIN_MAXENT, maxent_bounds)?,
        ParameterSpec::derived_from_str("mx01j", "mx01", maxent_bounds)?,
        ParameterSpec::derived_from_str("mx01y", "mx01", maxent_bounds)?,
        ParameterSpec::derived_from_str("mx01s", "mx01", maxent_bounds)?,
        ParameterSpec::derived_from_str("mx01v", "mx01", maxent_bounds)?,
        ParameterSpec::fixed("mx01r", 0.5, maxent_bounds)?,
        ParameterSpec::fixed("mf", 0.1, probability_bounds)?,
        ParameterSpec::fixed("dp", 1.0, probability_bounds)?,
        ParameterSpec::fixed("fdp", 0.0, probability_bounds)?,
    ];

    specs
        .into_iter()
        .map(|spec| {
            let transform = match spec.name() {
                "d" | "e" | "a" => ParameterTransform::Log,
                "j" | "ysv" | "ys" | "y" | "s" | "v" | "mx01" | "mx01j" | "mx01y" | "mx01s"
                | "mx01v" | "mx01r" | "mf" | "dp" | "fdp" => ParameterTransform::Logit,
                _ => ParameterTransform::Linear,
            };
            spec.with_transform(transform)
        })
        .collect()
}

fn configure_dec_j(specs: &mut [ParameterSpec]) -> Result<(), ParameterError> {
    replace_spec(
        specs,
        ParameterSpec::free(
            "j",
            BGB_MIN_MAXENT,
            ParameterBounds::new(BGB_MIN_CLADOGENESIS, 3.0 - BGB_MIN_CLADOGENESIS)?,
        )?,
    )?;
    Ok(())
}

fn configure_divalike(
    specs: &mut [ParameterSpec],
    founder_event_free: bool,
) -> Result<(), ParameterError> {
    let ysv_bounds = ParameterBounds::new(BGB_MIN_CLADOGENESIS, 2.0)?;
    let unit_bounds = ParameterBounds::new(BGB_MIN_CLADOGENESIS, 1.0)?;
    if founder_event_free {
        replace_spec(
            specs,
            ParameterSpec::free(
                "j",
                BGB_MIN_MAXENT,
                ParameterBounds::new(BGB_MIN_CLADOGENESIS, 2.0 - BGB_MIN_CLADOGENESIS)?,
            )?,
        )?;
    }
    replace_spec(
        specs,
        ParameterSpec::derived_from_str("ysv", "2-j", ysv_bounds)?,
    )?;
    replace_spec(
        specs,
        ParameterSpec::derived_from_str("ys", "ysv*1/2", unit_bounds)?,
    )?;
    replace_spec(
        specs,
        ParameterSpec::derived_from_str("y", "ysv*1/2", unit_bounds)?,
    )?;
    replace_spec(
        specs,
        ParameterSpec::derived_from_str("v", "ysv*1/2", unit_bounds)?,
    )?;
    replace_spec(specs, ParameterSpec::fixed("s", 0.0, unit_bounds)?)?;
    replace_spec(
        specs,
        ParameterSpec::fixed(
            "mx01v",
            0.5,
            ParameterBounds::new(BGB_MIN_MAXENT, BGB_MAX_MAXENT)?,
        )?,
    )?;
    Ok(())
}

fn configure_bayarealike(
    specs: &mut [ParameterSpec],
    founder_event_free: bool,
) -> Result<(), ParameterError> {
    let unit_bounds = ParameterBounds::new(BGB_MIN_CLADOGENESIS, 1.0)?;
    if founder_event_free {
        replace_spec(
            specs,
            ParameterSpec::free(
                "j",
                BGB_MIN_MAXENT,
                ParameterBounds::new(BGB_MIN_CLADOGENESIS, 1.0 - BGB_MIN_CLADOGENESIS)?,
            )?,
        )?;
    }
    replace_spec(
        specs,
        ParameterSpec::derived_from_str("ysv", "1-j", unit_bounds)?,
    )?;
    replace_spec(
        specs,
        ParameterSpec::derived_from_str("ys", "ysv*1/1", unit_bounds)?,
    )?;
    replace_spec(
        specs,
        ParameterSpec::derived_from_str("y", "1-j", unit_bounds)?,
    )?;
    replace_spec(specs, ParameterSpec::fixed("s", 0.0, unit_bounds)?)?;
    replace_spec(specs, ParameterSpec::fixed("v", 0.0, unit_bounds)?)?;
    replace_spec(
        specs,
        ParameterSpec::fixed(
            "mx01y",
            BGB_MAX_MAXENT,
            ParameterBounds::new(BGB_MIN_MAXENT, BGB_MAX_MAXENT)?,
        )?,
    )?;
    Ok(())
}

fn replace_spec(
    specs: &mut [ParameterSpec],
    replacement: ParameterSpec,
) -> Result<(), ParameterError> {
    let index = specs
        .iter()
        .position(|spec| spec.name == replacement.name)
        .expect("BioGeoBEARS preset replacement must name a default parameter");
    let transform = specs[index].transform;
    specs[index] = replacement.with_transform(transform)?;
    Ok(())
}

fn validate_parameter_name(name: String) -> Result<String, ParameterError> {
    let mut bytes = name.bytes();
    let valid_start = bytes
        .next()
        .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_');
    if !valid_start || !bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_') {
        return Err(ParameterError::InvalidParameterName { parameter: name });
    }
    Ok(name)
}

#[derive(Clone, Debug, PartialEq)]
pub enum ParameterError {
    InvalidParameterName {
        parameter: String,
    },
    InvalidBounds {
        min: f64,
        max: f64,
    },
    NonFiniteDeclaredValue {
        parameter: String,
        role: &'static str,
        value: f64,
    },
    InitialValueOutOfBounds {
        parameter: String,
        value: f64,
        bounds: ParameterBounds,
    },
    InitialValueNotStrictlyInsideBounds {
        parameter: String,
        value: f64,
        bounds: ParameterBounds,
        transform: ParameterTransform,
    },
    DegenerateFreeBounds {
        parameter: String,
        bounds: ParameterBounds,
    },
    InvalidTransformBounds {
        parameter: String,
        transform: ParameterTransform,
        bounds: ParameterBounds,
    },
    ExpressionParse {
        parameter: String,
        source: ParameterExpressionParseError,
    },
    DuplicateParameter {
        parameter: String,
    },
    UnknownParameter {
        parameter: String,
    },
    UnknownReference {
        parameter: String,
        reference: String,
    },
    CyclicDependency {
        cycle: Vec<String>,
    },
    FreeValueCountMismatch {
        expected: usize,
        actual: usize,
    },
    NonFiniteFreeValue {
        parameter: String,
        value: f64,
    },
    FreeValueOutOfBounds {
        parameter: String,
        value: f64,
        bounds: ParameterBounds,
    },
    DivisionByZero {
        parameter: String,
    },
    NonFiniteDerivedValue {
        parameter: String,
        value: f64,
    },
    DerivedValueOutOfBounds {
        parameter: String,
        value: f64,
        bounds: ParameterBounds,
    },
    MissingParameter {
        parameter: String,
    },
}

impl fmt::Display for ParameterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidParameterName { parameter } => write!(
                f,
                "parameter name '{parameter}' must be an ASCII identifier"
            ),
            Self::InvalidBounds { min, max } => write!(
                f,
                "parameter bounds must be finite and satisfy min <= max, got [{min}, {max}]"
            ),
            Self::NonFiniteDeclaredValue {
                parameter,
                role,
                value,
            } => write!(
                f,
                "parameter {parameter} {role} value must be finite, got {value}"
            ),
            Self::InitialValueOutOfBounds {
                parameter,
                value,
                bounds,
            } => write!(
                f,
                "parameter {parameter} initial value {value} is outside [{}, {}]",
                bounds.min, bounds.max
            ),
            Self::InitialValueNotStrictlyInsideBounds {
                parameter,
                value,
                bounds,
                transform,
            } => write!(
                f,
                "parameter {parameter} initial value {value} must be strictly inside ({}, {}) for {transform:?} optimization",
                bounds.min, bounds.max
            ),
            Self::DegenerateFreeBounds { parameter, bounds } => write!(
                f,
                "free parameter {parameter} must have min < max, got [{}, {}]",
                bounds.min, bounds.max
            ),
            Self::InvalidTransformBounds {
                parameter,
                transform,
                bounds,
            } => write!(
                f,
                "parameter {parameter} bounds [{}, {}] are invalid for {transform:?} optimization",
                bounds.min, bounds.max
            ),
            Self::ExpressionParse { parameter, source } => {
                write!(f, "invalid derived expression for {parameter}: {source}")
            }
            Self::DuplicateParameter { parameter } => {
                write!(f, "parameter table contains duplicate name '{parameter}'")
            }
            Self::UnknownParameter { parameter } => {
                write!(f, "parameter table does not contain '{parameter}'")
            }
            Self::UnknownReference {
                parameter,
                reference,
            } => write!(
                f,
                "derived parameter {parameter} references unknown parameter {reference}"
            ),
            Self::CyclicDependency { cycle } => {
                write!(f, "parameter dependency cycle: {}", cycle.join(" -> "))
            }
            Self::FreeValueCountMismatch { expected, actual } => write!(
                f,
                "free parameter vector has length {actual}, expected {expected}"
            ),
            Self::NonFiniteFreeValue { parameter, value } => {
                write!(f, "free parameter {parameter} must be finite, got {value}")
            }
            Self::FreeValueOutOfBounds {
                parameter,
                value,
                bounds,
            } => write!(
                f,
                "free parameter {parameter}={value} is outside [{}, {}]",
                bounds.min, bounds.max
            ),
            Self::DivisionByZero { parameter } => {
                write!(f, "derived parameter {parameter} divides by zero")
            }
            Self::NonFiniteDerivedValue { parameter, value } => write!(
                f,
                "derived parameter {parameter} resolved to non-finite value {value}"
            ),
            Self::DerivedValueOutOfBounds {
                parameter,
                value,
                bounds,
            } => write!(
                f,
                "derived parameter {parameter}={value} is outside [{}, {}]",
                bounds.min, bounds.max
            ),
            Self::MissingParameter { parameter } => {
                write!(f, "resolved parameter set does not contain '{parameter}'")
            }
        }
    }
}

impl Error for ParameterError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::ExpressionParse { source, .. } => Some(source),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounds(min: f64, max: f64) -> ParameterBounds {
        ParameterBounds::new(min, max).unwrap()
    }

    #[test]
    fn versioned_parameter_tables_round_trip_all_presets() {
        for preset in [
            BioGeoBearsPreset::Dec,
            BioGeoBearsPreset::DecJ,
            BioGeoBearsPreset::DivaLike,
            BioGeoBearsPreset::DivaLikeJ,
            BioGeoBearsPreset::BayAreaLike,
            BioGeoBearsPreset::BayAreaLikeJ,
        ] {
            let table = preset.parameter_table().unwrap();
            let serialized = table.to_versioned_tsv();
            assert!(serialized.starts_with("biogeo-parameter-table-v1\nname\tmode\t"));
            assert_eq!(parse_parameter_table(&serialized).unwrap(), table);
        }
    }

    #[test]
    fn versioned_parameter_table_parser_accepts_comments_bom_and_crlf() {
        let input = concat!(
            "# generated configuration\r\n",
            "\u{feff}biogeo-parameter-table-v1\r\n",
            "name\tmode\tvalue\tmin\tmax\ttransform\texpression\r\n",
            "d\tfree\t0.1\t0.000001\t2\tlog\t\r\n",
            "e\tfixed\t0.2\t0\t2\tlinear\t\r\n",
            "sum\tderived\t\t0\t4\tlinear\td+e\r\n",
        );
        let table = parse_parameter_table(input).unwrap();
        assert_eq!(table.free_parameter_names(), vec!["d"]);
        assert!((table.resolve_initial().unwrap().get("sum").unwrap() - 0.3).abs() < 1e-12);
    }

    #[test]
    fn versioned_parameter_table_parser_rejects_ambiguous_mode_fields() {
        let input = concat!(
            "biogeo-parameter-table-v1\n",
            "name\tmode\tvalue\tmin\tmax\ttransform\texpression\n",
            "d\tfree\t0.1\t0.000001\t2\tlog\td+1\n",
        );
        assert!(matches!(
            parse_parameter_table(input),
            Err(ParameterTableParseError::UnexpectedModeField {
                line: 3,
                mode,
                field: "expression",
            }) if mode == "free"
        ));

        let input = concat!(
            "biogeo-parameter-table-v1\n",
            "name\tmode\tvalue\tmin\tmax\ttransform\texpression\n",
            "d\tderived\t\t0\t2\tlinear\t\n",
        );
        assert!(matches!(
            parse_parameter_table(input),
            Err(ParameterTableParseError::MissingModeField {
                line: 3,
                mode,
                field: "expression",
            }) if mode == "derived"
        ));
    }

    #[test]
    fn expression_parser_respects_precedence_parentheses_and_scientific_notation() {
        let specs = vec![
            ParameterSpec::fixed("j", 0.5, bounds(0.0, 3.0)).unwrap(),
            ParameterSpec::derived_from_str("a", "3-j*2", bounds(-10.0, 10.0)).unwrap(),
            ParameterSpec::derived_from_str("b", "(3-j)*2", bounds(-10.0, 10.0)).unwrap(),
            ParameterSpec::derived_from_str("c", "1e-2 + -j", bounds(-10.0, 10.0)).unwrap(),
        ];
        let resolved = ParameterTable::new(specs)
            .unwrap()
            .resolve_initial()
            .unwrap();

        assert_eq!(resolved.get("a"), Some(2.0));
        assert_eq!(resolved.get("b"), Some(5.0));
        assert_eq!(resolved.get("c"), Some(-0.49));
    }

    #[test]
    fn rejects_arbitrary_calls_unknown_references_and_cycles() {
        assert!("system(cmd)".parse::<ParameterExpression>().is_err());

        let unknown = ParameterTable::new(vec![
            ParameterSpec::derived_from_str("a", "missing+1", bounds(0.0, 2.0)).unwrap(),
        ])
        .unwrap_err();
        assert_eq!(
            unknown,
            ParameterError::UnknownReference {
                parameter: "a".to_owned(),
                reference: "missing".to_owned(),
            }
        );

        let cycle = ParameterTable::new(vec![
            ParameterSpec::derived_from_str("a", "b+1", bounds(0.0, 2.0)).unwrap(),
            ParameterSpec::derived_from_str("b", "a+1", bounds(0.0, 2.0)).unwrap(),
        ])
        .unwrap_err();
        assert_eq!(
            cycle,
            ParameterError::CyclicDependency {
                cycle: vec!["a".to_owned(), "b".to_owned(), "a".to_owned()],
            }
        );
    }

    #[test]
    fn maps_ordered_free_vector_and_checks_bounds() {
        let table = ParameterTable::new(vec![
            ParameterSpec::free("d", 0.1, bounds(0.0, 1.0)).unwrap(),
            ParameterSpec::fixed("scale", 2.0, bounds(0.0, 3.0)).unwrap(),
            ParameterSpec::free("e", 0.2, bounds(0.0, 1.0)).unwrap(),
            ParameterSpec::derived_from_str("sum", "d+e", bounds(0.0, 2.0)).unwrap(),
        ])
        .unwrap();

        assert_eq!(table.free_parameter_names(), vec!["d", "e"]);
        assert_eq!(table.initial_free_values(), vec![0.1, 0.2]);
        let resolved = table.resolve_free_values(&[0.3, 0.4]).unwrap();
        assert_eq!(resolved.get("d"), Some(0.3));
        assert_eq!(resolved.get("e"), Some(0.4));
        assert!((resolved.get("sum").unwrap() - 0.7).abs() < 1e-12);

        assert!(matches!(
            table.resolve_free_values(&[1.1, 0.4]),
            Err(ParameterError::FreeValueOutOfBounds { parameter, .. }) if parameter == "d"
        ));
        assert_eq!(
            table.resolve_free_values(&[0.1]).unwrap_err(),
            ParameterError::FreeValueCountMismatch {
                expected: 2,
                actual: 1,
            }
        );
    }

    #[test]
    fn table_builder_can_fix_release_and_relink_parameters() {
        let table = biogeobears_default_parameter_table()
            .unwrap()
            .with_fixed("d", 0.25)
            .unwrap()
            .with_free("j", 0.2, bounds(1e-5, 2.99999))
            .unwrap()
            .with_derived_from_str("y", "(3-j)/3")
            .unwrap();
        let resolved = table.resolve_free_values(&[0.4, 0.6]).unwrap();

        assert_eq!(table.free_parameter_names(), vec!["e", "j"]);
        assert_eq!(resolved.get("d"), Some(0.25));
        assert_eq!(resolved.get("e"), Some(0.4));
        assert_eq!(resolved.get("j"), Some(0.6));
        assert!((resolved.get("y").unwrap() - 0.8).abs() < 1e-12);

        assert_eq!(
            table.with_fixed("not_a_parameter", 1.0).unwrap_err(),
            ParameterError::UnknownParameter {
                parameter: "not_a_parameter".to_owned(),
            }
        );
    }

    #[test]
    fn reports_only_free_parameters_reaching_likelihood_targets() {
        let table = biogeobears_default_parameter_table()
            .unwrap()
            .with_fixed("d", 0.1)
            .unwrap()
            .with_free("ysv", 2.0, bounds(1e-5, 3.0))
            .unwrap()
            .with_derived_from_str("y", "ysv/3")
            .unwrap()
            .with_free("mx01", 0.2, bounds(0.0001, 0.9999))
            .unwrap()
            .with_fixed("mx01y", 0.1)
            .unwrap()
            .with_fixed("mx01s", 0.1)
            .unwrap()
            .with_fixed("mx01v", 0.1)
            .unwrap()
            .with_fixed("mx01j", 0.1)
            .unwrap();

        assert_eq!(
            table
                .free_parameters_affecting(&["d", "e", "y", "s", "v", "j"])
                .unwrap(),
            vec!["e", "ysv"]
        );
        assert_eq!(
            table
                .free_parameters_affecting(&["mx01y", "mx01s", "mx01v", "mx01j"])
                .unwrap(),
            Vec::<&str>::new()
        );
    }

    #[test]
    fn optimization_transforms_are_validated_and_preserved_by_table_builders() {
        let table = biogeobears_default_parameter_table().unwrap();
        assert_eq!(
            table.spec("d").unwrap().transform(),
            ParameterTransform::Log
        );
        assert_eq!(
            table.spec("j").unwrap().transform(),
            ParameterTransform::Logit
        );
        assert_eq!(
            table.spec("x").unwrap().transform(),
            ParameterTransform::Linear
        );

        let updated = table
            .with_fixed("d", 0.2)
            .unwrap()
            .with_free("d", 0.1, bounds(1e-6, 2.0))
            .unwrap();
        assert_eq!(
            updated.spec("d").unwrap().transform(),
            ParameterTransform::Log
        );

        assert!(matches!(
            ParameterSpec::free("weight", 0.0, bounds(0.0, 1.0))
                .unwrap()
                .with_transform(ParameterTransform::Logit),
            Err(ParameterError::InitialValueNotStrictlyInsideBounds {
                parameter,
                ..
            }) if parameter == "weight"
        ));
        assert!(matches!(
            ParameterSpec::free("rate", 0.5, bounds(0.0, 1.0))
                .unwrap()
                .with_transform(ParameterTransform::Log),
            Err(ParameterError::InvalidTransformBounds { parameter, .. })
                if parameter == "rate"
        ));
    }

    #[test]
    fn rejects_degenerate_free_bounds_and_out_of_bounds_derived_values() {
        assert!(matches!(
            ParameterSpec::free("x", 1.0, bounds(1.0, 1.0)),
            Err(ParameterError::DegenerateFreeBounds { parameter, .. }) if parameter == "x"
        ));

        let error = ParameterTable::new(vec![
            ParameterSpec::fixed("source", 0.75, bounds(0.0, 1.0)).unwrap(),
            ParameterSpec::derived_from_str("double", "source*2", bounds(0.0, 1.0)).unwrap(),
        ])
        .unwrap_err();
        assert!(matches!(
            error,
            ParameterError::DerivedValueOutOfBounds {
                parameter,
                value: 1.5,
                ..
            } if parameter == "double"
        ));
    }

    #[test]
    fn fixed_values_may_sit_outside_optimization_bounds_like_bgb_defaults() {
        let spec = ParameterSpec::fixed("j", 0.0, bounds(1e-5, 3.0)).unwrap();
        let table = ParameterTable::new(vec![spec]).unwrap();

        assert_eq!(table.resolve_initial().unwrap().get("j"), Some(0.0));
    }

    #[test]
    fn detects_zero_division_at_resolution() {
        let error = ParameterTable::new(vec![
            ParameterSpec::fixed("denominator", 0.0, bounds(0.0, 1.0)).unwrap(),
            ParameterSpec::derived_from_str("ratio", "1/denominator", bounds(0.0, 1.0)).unwrap(),
        ])
        .unwrap_err();

        assert_eq!(
            error,
            ParameterError::DivisionByZero {
                parameter: "ratio".to_owned(),
            }
        );
    }

    #[test]
    fn default_biogeobears_table_has_all_23_rows_and_dec_links() {
        let table = biogeobears_default_parameter_table().unwrap();
        let names = table
            .specs()
            .iter()
            .map(ParameterSpec::name)
            .collect::<Vec<_>>();
        let resolved = table.resolve_free_values(&[0.1, 0.2]).unwrap();

        assert_eq!(names, BIOGEOBEARS_PARAMETER_NAMES);
        assert_eq!(table.free_parameter_names(), vec!["d", "e"]);
        assert_eq!(resolved.get("j"), Some(0.0));
        assert_eq!(resolved.get("ysv"), Some(3.0));
        assert_eq!(resolved.get("ys"), Some(2.0));
        assert_eq!(resolved.get("y"), Some(1.0));
        assert_eq!(resolved.get("s"), Some(1.0));
        assert_eq!(resolved.get("v"), Some(1.0));
        assert_eq!(resolved.get("mx01j"), Some(BGB_MIN_MAXENT));
    }

    #[test]
    fn plus_j_presets_release_d_e_j_in_stable_order() {
        for preset in [
            BioGeoBearsPreset::DecJ,
            BioGeoBearsPreset::DivaLikeJ,
            BioGeoBearsPreset::BayAreaLikeJ,
        ] {
            assert_eq!(
                preset.parameter_table().unwrap().free_parameter_names(),
                vec!["d", "e", "j"]
            );
        }
    }

    #[test]
    fn nested_presets_keep_official_links_when_j_is_fixed() {
        for preset in [BioGeoBearsPreset::DivaLike, BioGeoBearsPreset::BayAreaLike] {
            let table = preset.parameter_table().unwrap();
            assert!(matches!(
                table.spec("j").unwrap().mode(),
                ParameterMode::Fixed { value: 0.0 }
            ));
            for parameter in ["ysv", "ys", "y"] {
                assert!(
                    matches!(
                        table.spec(parameter).unwrap().mode(),
                        ParameterMode::Derived { .. }
                    ),
                    "{preset:?} should keep {parameter} linked"
                );
            }
        }
    }
}
