// Zero-dependency, deterministic Xoshiro256++ PRNG for reproducible fuzzing

#[derive(Clone, Debug)]
pub struct Rng {
    s: [u64; 4],
}

impl Rng {
    pub fn seed(seed: u64) -> Self {
        // SplitMix64 to initialize state array from a single 64-bit seed
        let mut sm = seed;
        let mut splitmix = || {
            sm = sm.wrapping_add(0x9E3779B97F4A7C15);
            let mut z = sm;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
            z ^ (z >> 31)
        };

        let s = [splitmix(), splitmix(), splitmix(), splitmix()];
        Self { s }
    }

    pub fn next_u64(&mut self) -> u64 {
        let result = (self.s[0].wrapping_add(self.s[3]))
            .rotate_left(23)
            .wrapping_add(self.s[0]);

        let t = self.s[1] << 17;

        self.s[2] ^= self.s[0];
        self.s[3] ^= self.s[1];
        self.s[1] ^= self.s[2];
        self.s[0] ^= self.s[3];

        self.s[2] ^= t;
        self.s[3] = self.s[3].rotate_left(45);

        result
    }

    pub fn gen_range(&mut self, min: usize, max: usize) -> usize {
        if min >= max {
            return min;
        }
        min + (self.next_u64() as usize % (max - min + 1))
    }

    pub fn gen_i64(&mut self, min: i64, max: i64) -> i64 {
        if min >= max {
            return min;
        }
        let diff = (max - min + 1) as u64;
        min + (self.next_u64() % diff) as i64
    }

    pub fn gen_bool(&mut self, p: f64) -> bool {
        let threshold = (p * (u32::MAX as f64)) as u32;
        (self.next_u64() as u32) <= threshold
    }

    pub fn choose<'a, T>(&mut self, slice: &'a [T]) -> &'a T {
        assert!(!slice.is_empty(), "Cannot choose from empty slice");
        let idx = self.gen_range(0, slice.len() - 1);
        &slice[idx]
    }
}
