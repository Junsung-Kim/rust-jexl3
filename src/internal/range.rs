// port of: org.apache.commons.jexl3.internal.IntegerRange and org.apache.commons.jexl3.internal.LongRange
use std::any::Any;
use crate::java::string::JString;

use crate::value::{HostObject, Value};

/// Which end the range iterates from (the Ascending / Descending nested classes).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Ascending,
    Descending,
}

/// The element width: IntegerRange or LongRange.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Width {
    Integer,
    Long,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Range {
    pub width: Width,
    pub direction: Direction,
    /// the low bound (IntegerRange.min)
    pub min: i64,
    /// the high bound (IntegerRange.max)
    pub max: i64,
}

impl Range {
    // port of: IntegerRange.create / LongRange.create
    pub fn create(width: Width, from: i64, to: i64) -> Range {
        if from <= to {
            Range { width, direction: Direction::Ascending, min: from, max: to }
        } else {
            Range { width, direction: Direction::Descending, min: to, max: from }
        }
    }

    pub fn get_min(&self) -> Value {
        self.wrap(self.min)
    }

    pub fn get_max(&self) -> Value {
        self.wrap(self.max)
    }

    fn wrap(&self, v: i64) -> Value {
        match self.width {
            Width::Integer => Value::Integer(v as i32),
            Width::Long => Value::Long(v),
        }
    }

    // port of: IntegerRange.size / LongRange.size
    pub fn size(&self) -> i32 {
        match self.width {
            Width::Integer => (self.max as i32).wrapping_sub(self.min as i32).wrapping_add(1),
            // LongRange.size() returns (int)(max - min + 1)
            Width::Long => self.max.wrapping_sub(self.min).wrapping_add(1) as i32,
        }
    }

    // port of: IntegerRange.contains / LongRange.contains
    pub fn contains(&self, v: &Value) -> bool {
        let n = match v {
            Value::Byte(b) => *b as i64,
            Value::Short(s) => *s as i64,
            Value::Integer(i) => *i as i64,
            // IntegerRange.contains narrows through intValue(), LongRange through longValue()
            Value::Long(l) => match self.width {
                Width::Integer => *l as i32 as i64,
                Width::Long => *l,
            },
            Value::Float(f) => match self.width {
                Width::Integer => *f as i32 as i64,
                Width::Long => *f as i64,
            },
            Value::Double(d) => match self.width {
                Width::Integer => *d as i32 as i64,
                Width::Long => *d as i64,
            },
            Value::BigInteger(b) => match self.width {
                Width::Integer => crate::java::number::big_integer_int_value(b) as i64,
                Width::Long => crate::java::number::big_integer_long_value(b),
            },
            Value::BigDecimal(b) => match self.width {
                Width::Integer => b.int_value() as i64,
                Width::Long => b.long_value(),
            },
            _ => return false,
        };
        self.min <= n && n <= self.max
    }

    /// The values the range iterates, in iteration order (IntegerRange.iterator /
    /// LongRange.iterator). NOTE: like Java's `cursor++` these iterators wrap around at
    /// Integer/Long.MAX_VALUE, so a range whose max is the type's maximum never ends.
    pub fn iter(&self) -> RangeIter<'_> {
        RangeIter {
            range: self,
            cursor: match self.direction {
                Direction::Ascending => self.min,
                Direction::Descending => self.max,
            },
        }
    }

    /// The Java class name relative to org.apache.commons.jexl3 (what the oracle reports).
    pub fn class_name(&self) -> String {
        format!(
            "internal.{}Range${}",
            match self.width {
                Width::Integer => "Integer",
                Width::Long => "Long",
            },
            match self.direction {
                Direction::Ascending => "Ascending",
                Direction::Descending => "Descending",
            }
        )
    }
}

/// The iterator of IntegerRange.Ascending / Descending (and the LongRange pair).
pub struct RangeIter<'a> {
    range: &'a Range,
    cursor: i64,
}

impl Iterator for RangeIter<'_> {
    type Item = Value;

    // port of: AscIntegerIterator / DescIntegerIterator (and the long pair): `hasNext` is a bound
    // check and `next` post-increments, so the cursor wraps at the type's extreme.
    fn next(&mut self) -> Option<Value> {
        match self.range.direction {
            Direction::Ascending => {
                if self.cursor > self.range.max {
                    return None;
                }
                let v = self.range.wrap(self.cursor);
                self.cursor = match self.range.width {
                    Width::Integer => (self.cursor as i32).wrapping_add(1) as i64,
                    Width::Long => self.cursor.wrapping_add(1),
                };
                Some(v)
            }
            Direction::Descending => {
                if self.cursor < self.range.min {
                    return None;
                }
                let v = self.range.wrap(self.cursor);
                self.cursor = match self.range.width {
                    Width::Integer => (self.cursor as i32).wrapping_sub(1) as i64,
                    Width::Long => self.cursor.wrapping_sub(1),
                };
                Some(v)
            }
        }
    }
}

impl HostObject for Range {
    fn class_name(&self) -> String {
        format!("org.apache.commons.jexl3.{}", Range::class_name(self))
    }

    // port of: IntegerRange.hashCode / LongRange.hashCode
    fn java_hash_code(&self) -> Option<i32> {
        // Java seeds this with getClass().hashCode(), an identity hash: the absolute number is not
        // reproducible even across two JVM runs. What IS observable, and what the hashCode/equals
        // contract needs, is that two equal ranges hash alike -- so the class contributes a stable
        // seed (its name's String.hashCode) instead of an address.
        let mut hash = JString::from(HostObject::class_name(self).as_str()).hash_code();
        let (min, max) = match self.width {
            Width::Integer => (self.min as i32, self.max as i32),
            Width::Long => (
                (self.min ^ ((self.min as u64 >> 32) as i64)) as i32,
                (self.max ^ ((self.max as u64 >> 32) as i64)) as i32,
            ),
        };
        hash = 13i32.wrapping_mul(hash).wrapping_add(min);
        hash = 13i32.wrapping_mul(hash).wrapping_add(max);
        Some(hash)
    }

    // port of: IntegerRange.equals / LongRange.equals
    fn java_equals(&self, other: &Value) -> Option<bool> {
        match other.as_host::<Range>() {
            Some(o) => Some(self.width == o.width && self.direction == o.direction && self.min == o.min && self.max == o.max),
            None => Some(false),
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
