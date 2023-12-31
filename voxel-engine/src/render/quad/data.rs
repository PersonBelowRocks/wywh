use super::isometric::QuadVertex;

#[derive(Copy, Clone, Hash, Debug, PartialEq, Eq)]
pub struct QData<T>([T; 4]);

impl<T> QData<T> {
    #[inline]
    pub fn filled(vertex: T) -> Self
    where
        T: Copy,
    {
        Self([vertex; 4])
    }

    #[inline]
    pub fn get(&self, vertex: QuadVertex) -> &T {
        &self.0[vertex.as_usize()]
    }

    #[inline]
    pub fn get_mut(&mut self, vertex: QuadVertex) -> &mut T {
        &mut self.0[vertex.as_usize()]
    }

    #[inline]
    pub fn inner(&self) -> &[T; 4] {
        &self.0
    }
}
