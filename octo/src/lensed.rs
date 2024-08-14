use std::mem::size_of;

pub const LENS_DIMS: usize = 8;
static_assertions::const_assert!(LENS_DIMS <= (u8::MAX as usize));
pub const LENS_SHAPE: (usize, usize, usize) = (LENS_DIMS, LENS_DIMS, LENS_DIMS);

fn usizes(i: [u8; 3]) -> (usize, usize, usize) {
    let [x, y, z] = i;
    (x as usize, y as usize, z as usize)
}

/// TODO: docs
#[derive(Copy, Clone)]
pub enum LensCell<T: Copy> {
    Shallow(T),
    Deep(u16),
}

impl<T: Copy> LensCell<T> {
    pub fn is_shallow(&self) -> bool {
        matches!(self, Self::Shallow(_))
    }

    pub fn is_deep(&self) -> bool {
        !self.is_shallow()
    }
}

/// TODO: docs
#[derive(Clone)]
pub struct Lens<T: Copy>([[[LensCell<T>; LENS_DIMS]; LENS_DIMS]; LENS_DIMS]);

impl<T: Copy> Lens<T> {
    /// Create a new shallow-filled lens of the given value.
    #[inline]
    #[must_use]
    pub fn new(value: T) -> Self {
        Self([[[LensCell::Shallow(value); LENS_DIMS]; LENS_DIMS]; LENS_DIMS])
    }

    /// Get the cell at the given position.
    ///
    /// ## Panics
    /// Panics if any of the indices exceed [`LENS_DIMS`]
    #[inline]
    #[must_use]
    pub fn get_cell(&self, cell: [u8; 3]) -> &LensCell<T> {
        let (i0, i1, i2) = usizes(cell);
        &self.0[i0][i1][i2]
    }

    /// Get the cell at the given position.
    ///
    /// ## Safety
    /// All cell index components must be less than [`LENS_DIMS`]
    #[inline]
    #[must_use]
    pub unsafe fn get_cell_unchecked(&self, cell: [u8; 3]) -> &LensCell<T> {
        let (i0, i1, i2) = usizes(cell);
        unsafe { self.0.get_unchecked(i0).get_unchecked(i1).get_unchecked(i2) }
    }

    /// Set the cell at the given position to a shallow value.
    ///
    /// ## Panics
    /// Panics if any of the indices exceed [`LENS_MAX_SIZE`]
    #[inline]
    pub fn set_cell_shallow(&mut self, cell: [u8; 3], value: T) {
        let (i0, i1, i2) = usizes(cell);
        self.0[i0][i1][i2] = LensCell::Shallow(value);
    }

    /// Set the cell at the given position to a shallow value.
    ///
    /// ## Safety
    /// All cell index components must be less than [`LENS_DIMS`]
    #[inline]
    pub unsafe fn set_cell_shallow_unchecked(&mut self, cell: [u8; 3], value: T) {
        let (i0, i1, i2) = usizes(cell);
        let slot = unsafe {
            self.0
                .get_unchecked_mut(i0)
                .get_unchecked_mut(i1)
                .get_unchecked_mut(i2)
        };

        *slot = LensCell::Shallow(value);
    }

    /// Set the cell at the given position to an index
    ///
    /// ## Panics
    /// Panics if any of the indices exceed [`LENS_MAX_SIZE`]
    #[inline]
    pub fn set_cell_deep(&mut self, cell: [u8; 3], index: u16) {
        let (i0, i1, i2) = usizes(cell);
        self.0[i0][i1][i2] = LensCell::Deep(index);
    }

    /// Set the cell at the given position to an index
    ///
    /// ## Safety
    /// All cell index components must be less than [`LENS_DIMS`]
    #[inline]
    pub unsafe fn set_cell_deep_unchecked(&mut self, cell: [u8; 3], index: u16) {
        let (i0, i1, i2) = usizes(cell);
        let slot = unsafe {
            self.0
                .get_unchecked_mut(i0)
                .get_unchecked_mut(i1)
                .get_unchecked_mut(i2)
        };

        *slot = LensCell::Deep(index);
    }
}

pub const PEB_DIMS: usize = 8;
static_assertions::const_assert!(PEB_DIMS <= (u8::MAX as usize));

pub const STORAGE_DIMS: u8 = (PEB_DIMS as u8) * (LENS_DIMS as u8);

#[derive(Clone)]
pub struct PaletteEntryBuffer<T: Copy>([[[T; PEB_DIMS]; PEB_DIMS]; PEB_DIMS]);

impl<T: Copy> PaletteEntryBuffer<T> {
    #[inline]
    pub fn new(value: T) -> Self {
        Self([[[value; PEB_DIMS]; PEB_DIMS]; PEB_DIMS])
    }

    #[inline]
    pub fn first(&self) -> &T {
        &self.0[0][0][0]
    }

    #[inline]
    pub fn get(&self, i: [u8; 3]) -> &T {
        let (i0, i1, i2) = usizes(i);

        &self.0[i0][i1][i2]
    }

    #[inline]
    pub unsafe fn get_unchecked(&self, i: [u8; 3]) -> &T {
        let (i0, i1, i2) = usizes(i);

        unsafe { self.0.get_unchecked(i0).get_unchecked(i1).get_unchecked(i2) }
    }

    #[inline]
    pub fn get_mut(&mut self, i: [u8; 3]) -> &mut T {
        let (i0, i1, i2) = usizes(i);

        &mut self.0[i0][i1][i2]
    }

    #[inline]
    pub unsafe fn get_mut_unchecked(&mut self, i: [u8; 3]) -> &mut T {
        let (i0, i1, i2) = usizes(i);

        unsafe {
            self.0
                .get_unchecked_mut(i0)
                .get_unchecked_mut(i1)
                .get_unchecked_mut(i2)
        }
    }
}

impl<T: Copy + Eq> PaletteEntryBuffer<T> {
    pub fn all_eq(&self) -> bool {
        self.0.iter().flatten().flatten().all(|e| e == self.first())
    }
}

/// TODO: docs
#[derive(Clone)]
pub struct PaletteEntry<T: Copy> {
    lens_cell: [u8; 3],
    buf: PaletteEntryBuffer<T>,
}

impl<T: Copy> PaletteEntry<T> {
    pub fn new(lens_cell: [u8; 3], value: T) -> Self {
        Self {
            lens_cell,
            buf: PaletteEntryBuffer::new(value),
        }
    }
}

pub const LENS_DIMS_LOG2: u8 = {
    let max_size = LENS_DIMS as u32;
    max_size.ilog2() as _
};

#[inline]
fn lens_index(i: [u8; 3]) -> [u8; 3] {
    i.map(|e| e >> LENS_DIMS_LOG2)
}

#[inline]
fn palette_buf_index(i: [u8; 3]) -> [u8; 3] {
    let pow = 0b1 << LENS_DIMS_LOG2;
    i.map(|e| e & (pow - 1))
}

#[inline]
fn is_in_bounds(i: [u8; 3]) -> bool {
    lens_index(i).into_iter().all(|e| e < LENS_DIMS as u8)
}

#[derive(Clone)]
pub struct LensedStorage<T: Copy> {
    lens: Lens<T>,
    palette: Vec<PaletteEntry<T>>,
}

impl<T: Copy> LensedStorage<T> {
    pub fn new(value: T) -> Self {
        Self {
            lens: Lens::new(value),
            palette: Vec::new(),
        }
    }

    pub fn with_capacity(value: T, capacity: usize) -> Self {
        Self {
            lens: Lens::new(value),
            palette: Vec::with_capacity(capacity),
        }
    }

    pub fn memory_usage(&self) -> usize {
        self.lens.0.len() * size_of::<u16>()
            + self.palette.capacity() * size_of::<PaletteEntry<T>>()
    }

    #[inline]
    pub fn get(&self, i: [u8; 3]) -> &T {
        let lens_index = lens_index(i);
        match self.lens.get_cell(lens_index) {
            LensCell::Shallow(value) => value,
            LensCell::Deep(deep_idx) => {
                let palette_buf_index = palette_buf_index(i);
                self.palette[*deep_idx as usize].buf.get(palette_buf_index)
            }
        }
    }

    #[inline]
    pub fn set(&mut self, i: [u8; 3], value: T) {
        let lens_index = lens_index(i);

        match self.get_palette_index(lens_index) {
            None => {
                let palette_buf_index = palette_buf_index(i);

                let peb = self.init_peb(lens_index).unwrap();

                let slot = peb.get_mut(palette_buf_index);
                *slot = value;
            }
            Some(palette_index) => {
                let palette_buf_index = palette_buf_index(i);

                let slot = self.palette[palette_index].buf.get_mut(palette_buf_index);
                *slot = value;
            }
        }
    }

    fn get_palette_index(&self, li: [u8; 3]) -> Option<usize> {
        match self.lens.get_cell(li) {
            LensCell::Shallow(_) => None,
            LensCell::Deep(deep_idx) => Some(*deep_idx as usize),
        }
    }

    #[inline]
    pub fn get_peb(&self, li: [u8; 3]) -> Option<&PaletteEntryBuffer<T>> {
        let palette_index = self.get_palette_index(li)?;
        Some(&self.palette[palette_index].buf)
    }

    #[inline]
    pub unsafe fn get_peb_unchecked(&self, li: [u8; 3]) -> Option<&PaletteEntryBuffer<T>> {
        let palette_index = self.get_palette_index(li)?;
        let entry = unsafe { self.palette.get_unchecked(palette_index) };
        Some(&entry.buf)
    }

    #[inline]
    pub fn get_peb_mut(&mut self, li: [u8; 3]) -> Option<&mut PaletteEntryBuffer<T>> {
        let palette_index = self.get_palette_index(li)?;
        Some(&mut self.palette[palette_index].buf)
    }

    #[inline]
    pub unsafe fn get_peb_mut_unchecked(
        &mut self,
        li: [u8; 3],
    ) -> Option<&mut PaletteEntryBuffer<T>> {
        let palette_index = self.get_palette_index(li)?;
        let entry = unsafe { self.palette.get_unchecked_mut(palette_index) };
        Some(&mut entry.buf)
    }

    #[inline]
    pub fn set_lens_cell_value(&mut self, li: [u8; 3], value: T) {
        self.lens.set_cell_shallow(li, value)
    }

    #[inline]
    pub fn init_peb(&mut self, li: [u8; 3]) -> Option<&mut PaletteEntryBuffer<T>> {
        match self.lens.get_cell(li) {
            LensCell::Shallow(value) => {
                let entry = PaletteEntry::new(li, *value);

                let palette_entry_index = self.palette.len() as u16;
                self.palette.push(entry);
                self.lens.set_cell_deep(li, palette_entry_index);

                Some(&mut self.palette[palette_entry_index as usize].buf)
            }
            LensCell::Deep(_) => None,
        }
    }

    pub fn compact_allocation(&mut self) {
        self.palette.shrink_to_fit()
    }
}

impl<T: Copy + Eq> LensedStorage<T> {
    #[inline]
    pub fn merge_and_cull(&mut self) {
        let mut new_palette = Vec::<PaletteEntry<T>>::with_capacity(self.palette.len());

        for entry in &self.palette {
            if self.lens.get_cell(entry.lens_cell).is_shallow() {
                continue;
            }

            if entry.buf.all_eq() {
                self.lens
                    .set_cell_shallow(entry.lens_cell, *entry.buf.first());
                continue;
            }

            let palette_index = new_palette.len() as u16;
            self.lens.set_cell_deep(entry.lens_cell, palette_index);

            new_palette.push(entry.clone())
        }

        self.palette = new_palette;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transformations() {
        assert_eq!(8, PEB_DIMS);
        assert_eq!(8, LENS_DIMS);

        assert_eq!([0, 0, 0], palette_buf_index([8, 8, 8]));
        assert_eq!([1, 1, 1], palette_buf_index([9, 9, 9]));
        assert_eq!([7, 7, 7], palette_buf_index([15, 15, 15]));
        assert_eq!([0, 0, 0], palette_buf_index([16, 16, 16]));

        assert_eq!([0, 0, 0], lens_index([0, 0, 0]));
        assert_eq!([0, 0, 0], lens_index([7, 7, 7]));
        assert_eq!([1, 1, 1], lens_index([8, 8, 8]));
        assert_eq!([1, 1, 1], lens_index([15, 15, 15]));
        assert_eq!([2, 2, 2], lens_index([16, 16, 16]));
    }

    #[test]
    fn io() {
        let mut s = LensedStorage::new(0);

        s.set([0, 0, 0], 42);
        s.set([63, 63, 63], 43);
        s.set([8, 8, 8], 1337);
        s.set([9, 9, 9], 1337);
        s.set([10, 10, 10], 404);

        assert_eq!(&42, s.get([0, 0, 0]));
        assert_eq!(&43, s.get([63, 63, 63]));
        assert_eq!(&1337, s.get([8, 8, 8]));
        assert_eq!(&1337, s.get([9, 9, 9]));
        assert_eq!(&404, s.get([10, 10, 10]));
        assert_eq!(&0, s.get([11, 11, 11]));
        assert_eq!(&0, s.get([16, 16, 16]));
    }

    #[test]
    fn lens_cell_ops() {
        let mut s = LensedStorage::new(0);

        assert!(s.get_peb([0, 0, 0]).is_none());
        assert_ne!(&42, s.get([0, 0, 0]));

        let peb = s.init_peb([0, 0, 0]).unwrap();
        *peb.get_mut([0, 0, 0]) = 42;
        *peb.get_mut([7, 7, 7]) = 43;

        let peb = s.init_peb([0, 1, 0]).unwrap();
        *peb.get_mut([4, 4, 4]) = 1337;

        assert_eq!(&42, s.get([0, 0, 0]));
        assert_eq!(&43, s.get([7, 7, 7]));

        assert_eq!(&1337, s.get([4, 12, 4]));
    }

    #[test]
    fn cleanup() {
        const BASE: u8 = 16;

        fn suite(s: &LensedStorage<i32>) {
            assert_eq!(&42, s.get([0, 0, 0]));
            assert_eq!(&43, s.get([1, 1, 1]));

            for x in 0..8 {
                for y in 0..8 {
                    for z in 0..8 {
                        let i = [BASE + x, BASE + y, BASE + z];
                        assert_eq!(&1337, s.get(i));
                    }
                }
            }

            assert_eq!(&1002, s.get([0, 12, 0]));
        }

        let mut s = LensedStorage::new(0);

        s.set([0, 0, 0], 42);
        s.set([1, 1, 1], 43);

        for x in 0..8 {
            for y in 0..8 {
                for z in 0..8 {
                    let i = [BASE + x, BASE + y, BASE + z];
                    s.set(i, 1337);
                }
            }
        }

        s.set([0, 12, 0], 1001);
        s.set_lens_cell_value([0, 1, 0], 1002);

        suite(&s);

        s.merge_and_cull();

        suite(&s);
    }
}
