//! The `improv` crate provides methods for generating random names and passwords, using the
//! eponymous [`names`] and [`passwords`] crates.
use cordial_guest::Guest;
use names::{Generator, Name};
use passwords::PasswordGenerator;
use polite::{FauxPas, Polite};
use serde::{Deserialize, Serialize};

/// The `Improv` struct produces randomized names and passwords.
pub struct Improv<'a> {
    /// The `name` field contains a [`Generator`] from the [`names`] crate.
    pub name: Generator<'a>,
    /// The `pass` field contains a [`PasswordGenerator`] from the [`passwords`] crate.
    pub pass: PasswordGenerator,
}

impl<'a> Improv<'a> {
    /// The `new` method creates an `Improv` struct, using the naming pattern [`Name::Numbered`] if
    /// `numbered` is `true`, and [`Name::Plain`] if `false`.
    pub fn new(numbered: bool) -> Self {
        let name = Improv::gen(numbered);
        let pass = PasswordGenerator::new();
        Self { name, pass }
    }

    fn gen(numbered: bool) -> Generator<'a> {
        match numbered {
            true => Generator::with_naming(Name::Numbered),
            false => Generator::with_naming(Name::Plain),
        }
    }

    pub fn from_pass(pass: PasswordGenerator, numbered: bool) -> Self {
        let name = Improv::gen(numbered);
        Self { name, pass }
    }

    /// The `name` method calls [`Generator::next`] to produce an [`Option`] where the [`Some`] variant contains a new name.  Commits a [`FauxPas`]
    /// if the [`Option`] is [`None`].
    pub fn name(&mut self) -> Polite<String> {
        let opt = self.name.next();
        match opt {
            Some(name) => Ok(name),
            None => Err(FauxPas::Improv("Failed to generate name.".to_string())),
        }
    }

    /// The `names` method calls [`Generator::next`] repeatedly to produce an [`Option`] where the [`Some`] variant contains a new name, entering successful names into a `String` vector.  Commits a [`FauxPas`]
    /// if any [`Option`] contains [`None`].
    pub fn names(&mut self, count: usize) -> Polite<Vec<String>> {
        let mut names = Vec::new();
        while names.len() < count {
            names.push(self.name()?)
        }
        Ok(names)
    }

    /// The `pass` method calls [`PasswordGenerator::generate_one`] to produce a single password.
    /// Not suitable for generating multiple passwords in a loop; use [`Improv::passes`] instead.  Commits
    /// a [`FauxPas`] if the [`passwords`] library returns an error, bubbling the message up.
    /// The `pass` field is public to expose the configuration
    /// methods available in the [`passwords`] crate for the [`PasswordGenerator`].
    pub fn pass(&self) -> Polite<String> {
        let pass = self.pass.generate_one();
        match pass {
            Ok(value) => Ok(value),
            Err(msg) => Err(FauxPas::Pass(msg.to_string())),
        }
    }

    /// The `passes` method calls [`PasswordGenerator::generate`] to produce a multiple passwords.
    /// Commits a [`FauxPas`] if the [`passwords`] library returns an error, bubbling the message up.
    /// The `pass` field is public to expose the configuration
    /// methods available in the [`passwords`] crate for the [`PasswordGenerator`].
    pub fn passes(&self, count: usize) -> Polite<Vec<String>> {
        let passes = self.pass.generate(count);
        match passes {
            Ok(values) => Ok(values),
            Err(msg) => Err(FauxPas::Pass(msg.to_string())),
        }
    }

    /// The `guest` method creates a new [`Guest`] struct using an improvised name and pass.
    /// Passes any [`FauxPas`] from the [`Improv::name`] and [`Improv::pass`] methods up.
    pub fn guest(&mut self) -> Polite<Guest> {
        let name = self.name()?;
        let hash = self.pass()?;
        Ok(Guest::new(&name, &hash))
    }

    /// The `guests` method creates a vector of type [`Guest`] and length `count`.  Passes any
    /// [`FauxPas`] from the [`Improv::names`] and [`Improv::passes`] methods up.  Optimized for creating multiple
    /// passwords at once, intended to facilitate development and testing.
    pub fn guests(&mut self, count: usize) -> Polite<Vec<Guest>> {
        let names = self.names(count)?;
        let passes = self.passes(count)?;
        let mut guests = Vec::new();
        let mut i = 0;
        while guests.len() < count {
            guests.push(Guest::new(&names[i], &passes[i]));
            i += 1;
        }
        Ok(guests)
    }
}

impl<'a> Default for Improv<'a> {
    fn default() -> Self {
        Self::new(true)
    }
}

/// The `Pass` struct holds configuration information for a [`PasswordGenerator`].
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
pub struct Pass {
    /// Wrapper for the `length` field in [`PasswordGenerator`].
    pub length: usize,
    /// Wrapper for the `numbers` field in [`PasswordGenerator`].
    pub numbers: bool,
    /// Wrapper for the `lowercase_letters` field in [`PasswordGenerator`].
    pub lowercase: bool,
    /// Wrapper for the `uppercase_letters` field in [`PasswordGenerator`].
    pub uppercase: bool,
    /// Wrapper for the `symbols` field in [`PasswordGenerator`].
    pub symbols: bool,
    /// Wrapper for the `spaces` field in [`PasswordGenerator`].
    pub spaces: bool,
    /// Wrapper for the `exclude_similar_characters` field in [`PasswordGenerator`].
    pub exclude: bool,
    /// Wrapper for the `strict` field in [`PasswordGenerator`].
    pub strict: bool,
}

impl Pass {
    /// Creates a new `Pass` struct from the default method.  Modify the fields directly after
    /// construction to customize.
    pub fn new() -> Self {
        Self::default()
    }
}

impl Default for Pass {
    fn default() -> Self {
        Self {
            length: 8,
            numbers: true,
            lowercase: true,
            uppercase: false,
            symbols: false,
            spaces: false,
            exclude: false,
            strict: false,
        }
    }
}
