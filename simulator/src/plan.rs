pub trait InteractionPlan<T> {
    #[must_use]
    fn new() -> Self;

    fn step(&mut self) -> Option<&T>;

    #[must_use]
    fn with_gen_interactions(self, count: u64) -> Self;

    fn gen_interactions(&mut self, count: u64);

    #[must_use]
    fn with_interaction(self, interaction: T) -> Self;

    fn add_interaction(&mut self, interaction: T);
}
