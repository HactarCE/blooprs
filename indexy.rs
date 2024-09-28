macro_rules! impl_index {
    (
        $( ($($generics:tt)*) )? $self:ident: $type:ty[$index:ident: $index_type:ty] -> $output_type:ty;
        $index_expr:expr;
        $index_mut_expr:expr;
    ) => {
        impl $( $($generics)* )? ::std::ops::Index<$index_type> for $type {
            type Output = $output_type;

            fn index(&$self, $index: $index_type) -> &Self::Output {
                $index_expr
            }
        }
        impl $( $($generics)* )? ::std::ops::IndexMut<$index_type> for $type {
            fn index_mut(&mut $self, $index: $index_type) -> &mut Self::Output {
                $index_mut_expr
            }
        }
    };
    (
        $( ($($generics:tt)*) )? $self:ident: $type:ty[$index:ident: $index_type:ty] -> $output_type:ty;
        $index_expr:expr;
    ) => {
        impl $( $($generics)* )? ::std::ops::Index<$index_type> for $type {
            type Output = $output_type;

            fn index(&$self, $index: $index_type) -> &Self::Output {
                $index_expr
            }
        }
    };
}
impl_index! {
    self: InputtKeyTracker[index: u7] -> u16;
    &self.keys[index.as_int() as usize];
    &mut self.keys[index.as_int() as usize];
}
