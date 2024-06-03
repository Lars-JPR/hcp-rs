/// indexed list is a strange name for a 2d array
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexedList<T> {
    data: Vec<T>,
    n_cols: usize,
}

impl<T> IndexedList<T> {
    pub fn new(n_cols: usize) -> Self {
        Self {
            data: Vec::new(),
            n_cols,
        }
    }

    pub fn flat(&self) -> &Vec<T> {
        &self.data
    }

    pub fn get(&self, row: usize, col: usize) -> Option<&T> {
        self.data.get(row * self.n_cols + col)
    }

    pub fn get_mut(&mut self, row: usize, col: usize) -> Option<&mut T> {
        self.data.get_mut(row * self.n_cols + col)
    }

    pub fn remove_row(&mut self, index: usize) {
        let pos = index * self.n_cols;
        self.data.drain(pos..(pos + self.n_cols));
    }
}

impl<T: Clone> IndexedList<T> {
    pub fn insert_row(&mut self, index: usize, element: &[T]) {
        let pos = index * self.n_cols;
        self.data.splice(pos..pos, element.iter().cloned());
    }

    pub fn push_row(&mut self, element: &[T]) {
        self.data.extend_from_slice(element);
    }
}

impl<T> std::ops::Index<usize> for IndexedList<T> {
    type Output = [T];
    fn index(&self, row: usize) -> &Self::Output {
        &self.data[row * self.n_cols..(row + 1) * self.n_cols]
    }
}

impl<T> std::ops::IndexMut<usize> for IndexedList<T> {
    fn index_mut(&mut self, row: usize) -> &mut Self::Output {
        &mut self.data[row * self.n_cols..(row + 1) * self.n_cols]
    }
}

impl<T> std::ops::Index<(usize, usize)> for IndexedList<T> {
    type Output = T;
    fn index(&self, (row, col): (usize, usize)) -> &Self::Output {
        &self.data[row * self.n_cols + col]
    }
}
impl<T> std::ops::IndexMut<(usize, usize)> for IndexedList<T> {
    fn index_mut(&mut self, (row, col): (usize, usize)) -> &mut Self::Output {
        &mut self.data[row * self.n_cols + col]
    }
}

impl<'a, T> FromIterator<&'a [T]> for IndexedList<T>
where
    T: Clone + 'a,
{
    fn from_iter<I: IntoIterator<Item = &'a [T]>>(iter: I) -> Self {
        let mut iter = iter.into_iter().peekable();
        let n_cols = iter.peek().map_or(0, |row| row.len());
        Self {
            data: iter.flatten().cloned().collect(),
            n_cols,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_iter() {
        let v: IndexedList<i32> = [1, 2, 3, 4, 5, 6].chunks(3).collect();
        assert_eq!(v[(0, 1)], 2);
        assert_eq!(v[(1, 2)], 6);
    }

    #[test]
    fn push() {
        let mut v = IndexedList::new(3);
        v.push_row(&[1, 2, 3]);
        v.push_row(&[4, 5, 6]);
        assert_eq!(v[(0, 1)], 2);
        assert_eq!(v[(1, 2)], 6);
    }
    #[test]
    fn insert() {
        let mut v: IndexedList<i32> = [1, 2, 3, 4, 5, 6].chunks(3).collect();
        v.insert_row(1, &[8, 8, 8]);
        assert_eq!(v[(1, 2)], 8);
    }
    #[test]
    fn remove() {
        let mut v: IndexedList<i32> = [1, 2, 3, 4, 5, 6].chunks(3).collect();
        v.remove_row(0);
        assert_eq!(v[(0, 1)], 5);
    }
}
