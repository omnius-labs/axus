use std::cmp::Ordering;

pub struct Kadex;

impl Kadex {
    pub fn find<'a>(base: &'a [u8], target: &'a [u8], elements: &[&'a [u8]], count: usize) -> Vec<&'a [u8]> {
        let mut list: Vec<SortEntry<'a>> = Vec::new();

        let diff: Vec<u8> = target.iter().zip(base).map(|(x, y)| x ^ y).collect();
        list.push(SortEntry { value: base, diff });

        for element in elements {
            let diff: Vec<u8> = target.iter().zip(*element).map(|(x, y)| x ^ y).collect();
            list.push(SortEntry { value: element.to_owned(), diff });
        }

        let mut results: Vec<&SortEntry<'a>> = Vec::with_capacity(count);

        // append dummy
        for _ in 0..count {
            results.push(&list[0]);
        }

        for entry in list.iter().skip(1) {
            let mut left = 0;
            let mut right = results.len();

            while left < right {
                let middle = (left + right) / 2;

                if Kadex::compare(&results[middle].diff, &entry.diff) != Ordering::Greater {
                    left = middle + 1;
                } else {
                    right = middle;
                }
            }

            if left == results.len() {
                continue;
            }

            for j in ((left + 1)..results.len()).rev() {
                results.swap(j - 1, j);
            }

            results[left] = entry;
        }

        results.into_iter().take_while(|v| v.value != base).map(|v| v.value).collect::<Vec<&'a [u8]>>()
    }

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

struct SortEntry<'a> {
    pub value: &'a [u8],
    pub diff: Vec<u8>,
}

#[cfg(test)]
mod tests {
    use std::cmp::Ordering;

    use super::Kadex;

    #[test]
    pub fn find_test() {
        let element1 = vec![1, 1, 1, 1];
        let element2 = vec![0, 1, 1, 1];
        let element3 = vec![0, 0, 1, 1];

        let base: Vec<u8> = vec![0, 0, 0, 0];
        let target: Vec<u8> = vec![1, 1, 1, 1];
        let elements: Vec<&[u8]> = vec![&element1, &element2, &element3];
        let res = Kadex::find(&base, &target, &elements, 3);
        assert_eq!(res, vec![&element1, &element2, &element3]);

        let base: Vec<u8> = vec![0, 0, 0, 0];
        let target: Vec<u8> = vec![1, 1, 1, 1];
        let elements: Vec<&[u8]> = vec![&element1, &element2, &element3];
        let res = Kadex::find(&base, &target, &elements, 2);
        assert_eq!(res, vec![&element1, &element2]);

        let base: Vec<u8> = vec![0, 0, 0, 0];
        let target: Vec<u8> = vec![1, 1, 1, 1];
        let elements: Vec<&[u8]> = vec![&element1, &element2, &element3];
        let res = Kadex::find(&base, &target, &elements, 1);
        assert_eq!(res, vec![&element1]);
    }

    #[test]
    pub fn find_farthest_first_test() {
        let candidates = [vec![1, 0], vec![0, 1], vec![1, 1], vec![0, 2], vec![0, 3]];
        let expected: Vec<&[u8]> = candidates.iter().map(Vec::as_slice).collect();
        let elements: Vec<&[u8]> = expected.iter().copied().rev().collect();
        let base = vec![255, 255];
        let target = vec![0, 0];

        for count in [1, 2, 3, 7] {
            let res = Kadex::find(&base, &target, &elements, count);
            assert_eq!(res, expected[..count.min(expected.len())]);
        }
    }

    #[test]
    pub fn find_shuffled_test() {
        let candidates = [vec![1, 0], vec![0, 1], vec![1, 1], vec![0, 2], vec![0, 3]];
        let expected: Vec<&[u8]> = candidates.iter().map(Vec::as_slice).collect();
        let elements = vec![expected[2], expected[4], expected[0], expected[3], expected[1]];
        let base = vec![255, 255];
        let target = vec![0, 0];

        for count in [1, 2, 3, 7] {
            let res = Kadex::find(&base, &target, &elements, count);
            assert_eq!(res, expected[..count.min(expected.len())]);
        }
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
