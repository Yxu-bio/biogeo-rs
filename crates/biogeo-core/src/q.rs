#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RateTransition {
    pub from: usize,
    pub to: usize,
    pub rate: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SparseQ {
    size: usize,
    transitions: Vec<RateTransition>,
    diagonal: Vec<f64>,
}

impl SparseQ {
    pub fn new(size: usize, transitions: Vec<RateTransition>) -> Self {
        let mut row_sums = vec![0.0; size];
        for transition in &transitions {
            row_sums[transition.from] += transition.rate;
        }

        let diagonal = row_sums.into_iter().map(|sum| -sum).collect();

        Self {
            size,
            transitions,
            diagonal,
        }
    }

    pub fn size(&self) -> usize {
        self.size
    }

    pub fn transitions(&self) -> &[RateTransition] {
        &self.transitions
    }

    pub fn diagonal(&self) -> &[f64] {
        &self.diagonal
    }

    pub fn off_diagonal_count(&self) -> usize {
        self.transitions.len()
    }

    pub fn max_exit_rate(&self) -> f64 {
        self.diagonal
            .iter()
            .map(|diagonal| -diagonal)
            .fold(0.0, f64::max)
    }

    pub fn to_dense_row_major(&self) -> Vec<f64> {
        let mut matrix = vec![0.0; self.size * self.size];
        for row in 0..self.size {
            matrix[row * self.size + row] = self.diagonal[row];
        }
        for transition in &self.transitions {
            matrix[transition.from * self.size + transition.to] += transition.rate;
        }
        matrix
    }

    pub fn row_sum(&self, row: usize) -> f64 {
        let off_diagonal_sum: f64 = self
            .transitions
            .iter()
            .filter(|transition| transition.from == row)
            .map(|transition| transition.rate)
            .sum();

        self.diagonal[row] + off_diagonal_sum
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dense_export_includes_diagonal_and_transitions() {
        let q = SparseQ::new(
            2,
            vec![
                RateTransition {
                    from: 0,
                    to: 1,
                    rate: 0.25,
                },
                RateTransition {
                    from: 1,
                    to: 0,
                    rate: 0.5,
                },
            ],
        );

        assert_eq!(q.diagonal(), &[-0.25, -0.5]);
        assert_eq!(q.max_exit_rate(), 0.5);
        assert_eq!(q.to_dense_row_major(), vec![-0.25, 0.25, 0.5, -0.5]);
    }
}
