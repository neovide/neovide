use num::{cast::AsPrimitive, Integer};
use std::{
    clone::Clone,
    ops::{Bound, Index, IndexMut, Range, RangeBounds},
};

/// A simple ring buffer data structure
/// The buffer is always full and wraps around so that the oldest elements are overwritten.
/// It supports both negative and positive indexing and also indexing past the size.
pub struct RingBuffer<T> {
    elements: Vec<T>,
    current_index: isize,
}

pub struct RingBufferIter<'a, T> {
    ring_buffer: &'a RingBuffer<T>,
    range: Range<isize>,
}

pub struct RingBufferIterMut<'a, T> {
    ring_buffer: &'a mut RingBuffer<T>,
    range: Range<isize>,
}

impl<T: Clone> RingBuffer<T> {
    pub fn new(size: usize, default_value: T) -> Self {
        let mut elements = Vec::new();
        elements.resize(size, default_value);
        Self {
            current_index: 0,
            elements,
        }
    }

    pub fn clone_from_iter<'a, I>(&'a mut self, iter: I)
    where
        I: IntoIterator<Item = &'a T>,
    {
        self.iter_mut().zip(iter).for_each(|(a, b)| *a = b.clone());
    }

    pub fn is_empty(&self) -> bool {
        self.elements.is_empty()
    }

    pub fn iter(&self) -> RingBufferIter<'_, T> {
        self.iter_range(..)
    }

    pub fn iter_mut(&mut self) -> RingBufferIterMut<'_, T> {
        self.iter_range_mut(..)
    }

    pub fn iter_range<R: RangeBounds<isize>>(&self, range: R) -> RingBufferIter<'_, T> {
        let range = self.get_bounds(range);
        RingBufferIter {
            ring_buffer: self,
            range,
        }
    }

    pub fn iter_range_mut<R: RangeBounds<isize>>(&mut self, range: R) -> RingBufferIterMut<'_, T> {
        let range = self.get_bounds(range);
        RingBufferIterMut {
            ring_buffer: self,
            range,
        }
    }

    pub fn len(&self) -> usize {
        self.elements.len()
    }

    pub fn resize(&mut self, new_size: usize, default_value: T) {
        if new_size > 0 && !self.elements.is_empty() {
            let index = self.get_array_index(0);
            self.elements.rotate_left(index);
        }
        self.elements.resize(new_size, default_value);
        self.current_index = 0;
    }

    pub fn rotate(&mut self, num: isize) {
        self.current_index += num;
    }

    fn get_array_index(&self, index: isize) -> usize {
        let num = self.elements.len() as isize;
        (self.current_index + index).rem_euclid(num) as usize
    }

    fn get_bounds<R: RangeBounds<isize>>(&self, range: R) -> Range<isize> {
        let start = match range.start_bound() {
            Bound::Included(start) => *start,
            Bound::Excluded(start) => *start + 1,
            Bound::Unbounded => 0,
        };
        let end = match range.end_bound() {
            Bound::Included(end) => *end + 1,
            Bound::Excluded(end) => *end,
            Bound::Unbounded => self.len() as isize,
        };
        start..end
    }
}

impl<T: Clone, I: Integer + AsPrimitive<isize>> Index<I> for RingBuffer<T> {
    type Output = T;

    fn index(&self, index: I) -> &Self::Output {
        let array_index = self.get_array_index(index.as_());
        &self.elements[array_index]
    }
}

impl<T: Clone, I: Integer + AsPrimitive<isize>> IndexMut<I> for RingBuffer<T> {
    fn index_mut(&mut self, index: I) -> &mut Self::Output {
        let array_index = self.get_array_index(index.as_());
        &mut self.elements[array_index]
    }
}

impl<'a, T: Clone> Iterator for RingBufferIter<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.range.is_empty() {
            return None;
        }

        let ret = &self.ring_buffer[self.range.start];
        self.range.start += 1;
        Some(ret)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.range.size_hint()
    }
}

impl<'a, T: Clone> Iterator for RingBufferIterMut<'a, T> {
    type Item = &'a mut T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.range.is_empty() {
            return None;
        }
        let elements = self.ring_buffer.elements.as_mut_ptr();
        let array_index = self.ring_buffer.get_array_index(self.range.start);
        let ret = unsafe { &mut *elements.add(array_index) };
        self.range.start += 1;
        Some(ret)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.range.size_hint()
    }
}

impl<'a, T: Clone> IntoIterator for &'a RingBuffer<T> {
    type Item = &'a T;

    type IntoIter = RingBufferIter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a, T: Clone> IntoIterator for &'a mut RingBuffer<T> {
    type Item = &'a mut T;

    type IntoIter = RingBufferIterMut<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty() {
        let mut buffer = RingBuffer::<i32>::new(0, 5);
        assert_eq!(buffer.len(), 0);
        assert!(buffer.is_empty());
        assert_eq!(buffer.iter().size_hint(), (0, Some(0)));
        assert_eq!(buffer.iter_mut().size_hint(), (0, Some(0)));
    }

    #[test]
    fn single_element() {
        let mut buffer = RingBuffer::<i32>::new(1, 5);
        assert_eq!(buffer.len(), 1);
        assert!(!buffer.is_empty());
        assert_eq!(buffer[0], 5);
        buffer[0] = 3;
        assert_eq!(buffer[0], 3);
        assert!(buffer.iter().eq([3].iter()));
        buffer.clone_from_iter(&[7]);
        assert!(buffer.iter().eq([7].iter()));
        let mut iter = buffer.iter();
        assert_eq!(iter.size_hint(), (1, Some(1)));
        iter.next();
        assert_eq!(iter.size_hint(), (0, Some(0)));
        let mut iter = buffer.iter_mut();
        assert_eq!(iter.size_hint(), (1, Some(1)));
        iter.next();
        assert_eq!(iter.size_hint(), (0, Some(0)));
    }

    #[test]
    fn three_elements() {
        let mut buffer = RingBuffer::<i32>::new(3, 0);
        assert_eq!(buffer.len(), 3);
        assert_eq!(buffer[0], 0);
        assert_eq!(buffer[2], 0);
        buffer.clone_from_iter(&[1, 2, 3]);
        assert!(buffer.iter().eq([1, 2, 3].iter()));
        assert_eq!(buffer[0], 1);
        assert_eq!(buffer[2], 3);
    }

    #[test]
    fn rotate_forwards() {
        let mut buffer = RingBuffer::<i32>::new(5, 0);
        buffer.clone_from_iter(&[1, 2, 3, 4, 5]);
        buffer.rotate(2);
        assert_eq!(buffer[0], 3);
        assert_eq!(buffer[4], 2);
        assert!(buffer.iter().eq([3, 4, 5, 1, 2].iter()));
        assert_eq!(buffer[-2], 1);
        buffer.clone_from_iter(&[5, 6, 7, 8, 9]);
        assert!(buffer.iter().eq([5, 6, 7, 8, 9].iter()));
    }

    #[test]
    fn rotate_backwards() {
        let mut buffer = RingBuffer::<i32>::new(3, 0);
        assert!(buffer.iter().eq([0, 0, 0].iter()));
        buffer[0] = 0;
        buffer[1] = 2;
        buffer[2] = 5;
        buffer.rotate(-1);
        assert!(buffer.iter().eq([5, 0, 2].iter()));
        assert_eq!(buffer[-1], 2);
        buffer.clone_from_iter(&[5, 6, 7]);
        assert!(buffer.iter().eq([5, 6, 7].iter()));
    }

    #[test]
    fn resize_bigger() {
        let mut buffer = RingBuffer::<i32>::new(3, 0);
        assert!(buffer.iter().eq([0, 0, 0].iter()));
        buffer[0] = 0;
        buffer[1] = 2;
        buffer[2] = 5;
        buffer.rotate(-1);
        buffer.resize(5, 7);
        assert!(buffer.iter().eq([5, 0, 2, 7, 7].iter()));
    }

    #[test]
    fn resize_smaller() {
        let mut buffer = RingBuffer::<i32>::new(3, 0);
        assert!(buffer.iter().eq([0, 0, 0].iter()));
        buffer[0] = 0;
        buffer[1] = 2;
        buffer[2] = 5;
        buffer.rotate(1);
        buffer.resize(2, 7);
        assert!(buffer.iter().eq([2, 5].iter()));
    }

    #[test]
    fn iter_range() {
        let mut buffer = RingBuffer::<i32>::new(5, 0);
        buffer.clone_from_iter(&[1, 2, 3, 4, 5]);
        assert!(buffer.iter_range(1..3).eq([2, 3].iter()));
        assert!(buffer.iter_range(-1..1).eq([5, 1].iter()));
    }
}
