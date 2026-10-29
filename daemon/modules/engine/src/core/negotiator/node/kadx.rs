use std::cmp::Ordering;

pub struct Kadex;

impl Kadex {
    /// Orders two XOR distances, nearest first.
    ///
    /// The distance is compared from its last byte toward its first, because the node ID and the AssetKey hash are interpreted the same way by every node; another implementation must use this order to choose the same closest nodes.
    pub fn compare(x: &[u8], y: &[u8]) -> Ordering {
        if x.len() != y.len() {
            return x.len().cmp(&y.len());
        }

        if x.is_empty() {
            return Ordering::Equal;
        }

        for i in (0..x.len()).rev() {
            let o = x[i].cmp(&y[i]);
            if o != Ordering::Equal {
                return o;
            }
        }

        Ordering::Equal
    }
}

#[cfg(test)]
mod tests {
    use std::cmp::Ordering;

    use super::Kadex;

    #[test]
    fn the_nearest_distance_sorts_first() {
        let target = vec![1u8, 1];
        let near = vec![1u8, 0];
        let far = vec![0u8, 0];
        let distance = |id: &Vec<u8>| -> Vec<u8> { target.iter().zip(id).map(|(left, right)| left ^ right).collect() };

        assert_eq!(Kadex::compare(&distance(&near), &distance(&far)), Ordering::Less);
    }

    #[test]
    pub fn compare_test() {
        let x: Vec<u8> = vec![1, 1, 1, 1];
        let y: Vec<u8> = vec![1];
        let res = Kadex::compare(&x, &y);
        assert_eq!(res, Ordering::Greater);

        let x: Vec<u8> = vec![];
        let y: Vec<u8> = vec![];
        let res = Kadex::compare(&x, &y);
        assert_eq!(res, Ordering::Equal);

        let x: Vec<u8> = vec![0, 0, 0, 0];
        let y: Vec<u8> = vec![0, 0, 0, 1];
        let res = Kadex::compare(&x, &y);
        assert_eq!(res, Ordering::Less);
    }
}
