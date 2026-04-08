use std::iter;

pub fn peek_while(
    chars: &mut std::iter::Peekable<impl Iterator<Item = (usize, char)>>,
    condition: impl Fn(&(usize, char)) -> bool,
) -> String {
    iter::from_fn(|| chars.next_if(&condition))
        .map(|(_, c)| c)
        .collect()
}
