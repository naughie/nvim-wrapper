use std::borrow::Cow;
use std::ffi::OsStr;
use std::fmt;
use std::marker::PhantomData;
use std::path::Path;

use serde::de::{DeserializeSeed, IgnoredAny, MapAccess, Visitor};
use serde::{Deserialize, Deserializer};

fn config_file(path: Option<&Path>) -> Option<String> {
    if let Some(path) = path {
        std::fs::read_to_string(path).ok()
    } else {
        std::env::home_dir().and_then(|mut path| {
            path.push(".config");
            path.push("nvim-wrapper.toml");

            std::fs::read_to_string(path).ok()
        })
    }
}

#[derive(Clone, Debug, Deserialize)]
struct NvimPath(Cow<'static, str>);
#[derive(Clone, Debug, Default)]
pub struct LocalNvimPath(NvimPath);
#[derive(Debug, Default)]
pub struct RemoteNvimPath(NvimPath);

impl AsRef<OsStr> for LocalNvimPath {
    fn as_ref(&self) -> &OsStr {
        OsStr::new(&*self.0.0)
    }
}
impl PartialEq<str> for LocalNvimPath {
    fn eq(&self, other: &str) -> bool {
        self.0.0 == other
    }
}
impl PartialEq<str> for RemoteNvimPath {
    fn eq(&self, other: &str) -> bool {
        self.0.0 == other
    }
}

impl RemoteNvimPath {
    pub fn as_prog(&self) -> impl openrussh_client::process::Arg {
        &*self.0.0
    }
}

impl Default for NvimPath {
    fn default() -> Self {
        Self(Cow::Borrowed("nvim"))
    }
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct NvimConfig {
    nvim: NvimPath,
}

#[derive(Default)]
pub struct LocalNvimConfig {
    pub nvim: LocalNvimPath,
}
#[derive(Default)]
pub struct RemoteNvimConfig {
    pub nvim: RemoteNvimPath,
}

impl LocalNvimConfig {
    fn from_conf(config: NvimConfig) -> Self {
        Self {
            nvim: LocalNvimPath(config.nvim),
        }
    }
}

impl RemoteNvimConfig {
    fn from_conf(config: NvimConfig) -> Self {
        Self {
            nvim: RemoteNvimPath(config.nvim),
        }
    }
}

#[derive(Clone, Copy)]
struct FieldMatches<'a, const N: usize>([&'a str; N]);

impl<'de, const N: usize> DeserializeSeed<'de> for FieldMatches<'_, N> {
    type Value = u8;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(self)
    }
}

impl<const N: usize> Visitor<'_> for FieldMatches<'_, N> {
    type Value = u8;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a table key")
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E> {
        println!("visit: self = {:?}, value = {value}", self.0);
        for idx in 0..N {
            if self.0[idx] == value {
                return Ok(idx as u8);
            }
        }
        Ok(u8::MAX)
    }
}

#[derive(Clone, Copy)]
struct SelectField<'a, S> {
    field: &'a str,
    seed: S,
}

impl<'de, S> DeserializeSeed<'de> for SelectField<'_, S>
where
    S: DeserializeSeed<'de>,
    S::Value: Default,
{
    type Value = S::Value;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_map(self)
    }
}

impl<'de, S> Visitor<'de> for SelectField<'_, S>
where
    S: DeserializeSeed<'de>,
    S::Value: Default,
{
    type Value = S::Value;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a table")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        while let Some(matches) = map.next_key_seed(FieldMatches([self.field]))? {
            if matches == 0 {
                let value = map.next_value_seed(self.seed)?;
                while map.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {}
                return Ok(value);
            }
            map.next_value::<IgnoredAny>()?;
        }
        Ok(Self::Value::default())
    }
}

struct SelectFields<'a, S, T> {
    first: SelectField<'a, S>,
    second: SelectField<'a, T>,
}

impl<'de, S, T> DeserializeSeed<'de> for SelectFields<'_, S, T>
where
    S: DeserializeSeed<'de> + Copy,
    S::Value: Default,
    T: DeserializeSeed<'de, Value = S::Value> + Copy,
{
    type Value = [S::Value; 2];

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_map(self)
    }
}

impl<'de, S, T> Visitor<'de> for SelectFields<'_, S, T>
where
    S: DeserializeSeed<'de> + Copy,
    S::Value: Default,
    T: DeserializeSeed<'de, Value = S::Value> + Copy,
{
    type Value = [S::Value; 2];

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a table")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        println!(
            "first.field = {}, second.field = {}",
            self.first.field, self.second.field
        );
        let mut first: Option<S::Value> = None;
        let mut second: Option<S::Value> = None;

        while let Some(matches) =
            map.next_key_seed(FieldMatches([self.first.field, self.second.field]))?
        {
            println!("matches = {matches}");
            if matches == 0 {
                let value = map.next_value_seed(self.first.seed)?;
                if let Some(other) = second {
                    while map.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {}
                    return Ok([value, other]);
                }
                first = Some(value);
            } else if matches == 1 {
                let value = map.next_value_seed(self.second.seed)?;
                while map.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {}
                if let Some(other) = first {
                    while map.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {}
                    return Ok([other, value]);
                }
                second = Some(value);
            } else {
                map.next_value::<IgnoredAny>()?;
            }
        }
        while map.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {}
        Ok([first.unwrap_or_default(), second.unwrap_or_default()])
    }
}

fn parse_local_config(toml: &str) -> LocalNvimConfig {
    struct LocalConfigImpl(NvimConfig);

    impl<'de> Deserialize<'de> for LocalConfigImpl {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
        {
            SelectField {
                field: "local",
                seed: PhantomData,
            }
            .deserialize(deserializer)
            .map(Self)
        }
    }

    toml::from_str::<LocalConfigImpl>(toml)
        .map(|config| LocalNvimConfig::from_conf(config.0))
        .unwrap_or_default()
}

pub fn get_local_config(config_path: Option<&Path>) -> LocalNvimConfig {
    config_file(config_path)
        .map(|toml| parse_local_config(&toml))
        .unwrap_or_default()
}

#[derive(Default)]
pub struct DualConfig {
    pub local: LocalNvimConfig,
    pub remote: RemoteNvimConfig,
}

fn parse_dual_config(toml: &str, host: &str) -> DualConfig {
    #[derive(Default)]
    struct DualConfigImpl {
        local: NvimConfig,
        remote: NvimConfig,
    }

    struct DualConfigSeed<'a>(&'a str);

    impl<'de> DeserializeSeed<'de> for DualConfigSeed<'_> {
        type Value = DualConfigImpl;

        fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
        where
            D: Deserializer<'de>,
        {
            SelectFields {
                first: SelectField {
                    field: "local",
                    seed: PhantomData,
                },
                second: SelectField {
                    field: "remote",
                    seed: SelectField {
                        field: self.0,
                        seed: PhantomData,
                    },
                },
            }
            .deserialize(deserializer)
            .map(|[local, remote]| DualConfigImpl { local, remote })
        }
    }

    toml::de::Deserializer::parse(toml)
        .and_then(|deserializer| DualConfigSeed(host).deserialize(deserializer))
        .map(|config| DualConfig {
            local: LocalNvimConfig::from_conf(config.local),
            remote: RemoteNvimConfig::from_conf(config.remote),
        })
        .unwrap_or_default()
}

pub fn get_dual_config(config_path: Option<&Path>, host: &str) -> DualConfig {
    config_file(config_path)
        .map(|toml| parse_dual_config(&toml, host))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_config() {
        let toml = r#"
                [local]
                nvim = "/usr/local/bin/nvim"
                [remote.my-host1]
                nvim = "/opt/bin/nvim"
                [remote.my-host2]
                nvim = ""
            "#;

        assert_eq!(&parse_local_config(toml).nvim, "/usr/local/bin/nvim");

        assert_eq!(
            &parse_dual_config(toml, "my-host1").local.nvim,
            "/usr/local/bin/nvim"
        );
        assert_eq!(
            &parse_dual_config(toml, "my-host1").remote.nvim,
            "/opt/bin/nvim"
        );

        assert_eq!(
            &parse_dual_config(toml, "my-host2").local.nvim,
            "/usr/local/bin/nvim"
        );
        assert_eq!(&parse_dual_config(toml, "my-host2").remote.nvim, "");

        let toml = r#"
                [local]
                [remote.my-host1]
            "#;

        assert_eq!(&parse_local_config(toml).nvim, "nvim");
        assert_eq!(&parse_dual_config(toml, "my-host1").local.nvim, "nvim");
        assert_eq!(&parse_dual_config(toml, "my-host1").remote.nvim, "nvim");
        assert_eq!(&parse_dual_config(toml, "my-host2").local.nvim, "nvim");
        assert_eq!(&parse_dual_config(toml, "my-host2").remote.nvim, "nvim");

        let toml = r#"
                ["lo\u0063al"]
                nvim = "local"
                ["re\u006dote"."my\u002dhost"]
                nvim = "remote"
            "#;
        assert_eq!(&parse_local_config(toml).nvim, "local");
        assert_eq!(&parse_dual_config(toml, "my-host").local.nvim, "local");
        assert_eq!(&parse_dual_config(toml, "my-host").remote.nvim, "remote");
    }
}
