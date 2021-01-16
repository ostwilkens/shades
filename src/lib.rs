#![feature(min_const_generics)]
#![cfg_attr(feature = "fun-call", feature(unboxed_closures), feature(fn_traits))]

pub mod writer;

use std::{iter::once, marker::PhantomData, ops};

#[derive(Debug)]
pub struct Shader {
  pub(crate) decls: Vec<ShaderDecl>,
  next_fun_handle: u16,
  next_global_handle: u16,
}

impl Shader {
  pub fn new_vertex_shader(f: impl FnOnce(&mut Self, VertexShaderEnv)) -> Self {
    let mut shader = Self::new();
    f(&mut shader, VertexShaderEnv::new());
    shader
  }

  pub fn new_tess_ctrl_shader(f: impl FnOnce(&mut Self, TessCtrlShaderEnv)) -> Self {
    let mut shader = Self::new();
    f(&mut shader, TessCtrlShaderEnv::new());
    shader
  }

  pub fn new_tess_eval_shader(f: impl FnOnce(&mut Self, TessEvalShaderEnv)) -> Self {
    let mut shader = Self::new();
    f(&mut shader, TessEvalShaderEnv::new());
    shader
  }

  pub fn new_geometry_shader(f: impl FnOnce(&mut Self, GeometryShaderEnv)) -> Self {
    let mut shader = Self::new();
    f(&mut shader, GeometryShaderEnv::new());
    shader
  }

  pub fn new_fragment_shader(f: impl FnOnce(&mut Self, FragmentShaderEnv)) -> Self {
    let mut shader = Self::new();
    f(&mut shader, FragmentShaderEnv::new());
    shader
  }

  fn new() -> Self {
    Self {
      decls: Vec::new(),
      next_fun_handle: 0,
      next_global_handle: 0,
    }
  }

  pub fn fun<F, R, A>(&mut self, f: F) -> FunHandle<R, A>
  where
    F: ToFun<R, A>,
  {
    let fundef = f.build_fn();
    let handle = self.next_fun_handle;
    self.next_fun_handle += 1;

    self.decls.push(ShaderDecl::FunDef(handle, fundef.erased));

    FunHandle {
      erased: ErasedFunHandle::UserDefined(handle as _),
      _phantom: PhantomData,
    }
  }

  pub fn main_fun<F, R>(&mut self, f: F) -> FunHandle<R, ()>
  where
    F: ToFun<R, ()>,
  {
    let fundef = f.build_fn();

    self.decls.push(ShaderDecl::Main(fundef.erased));

    FunHandle {
      erased: ErasedFunHandle::Main,
      _phantom: PhantomData,
    }
  }

  pub fn constant<T>(&mut self, expr: Expr<T>) -> Var<T>
  where
    T: ToType,
  {
    let handle = self.next_global_handle;
    self.next_global_handle += 1;

    self
      .decls
      .push(ShaderDecl::Const(handle, T::ty(), expr.erased));

    Var::new(ScopedHandle::global(handle))
  }

  pub fn input<T>(&mut self) -> Var<T>
  where
    T: ToType,
  {
    let handle = self.next_global_handle;
    self.next_global_handle += 1;

    self.decls.push(ShaderDecl::In(handle, T::ty()));

    Var::new(ScopedHandle::global(handle))
  }

  pub fn output<T>(&mut self) -> Var<T>
  where
    T: ToType,
  {
    let handle = self.next_global_handle;
    self.next_global_handle += 1;

    self.decls.push(ShaderDecl::Out(handle, T::ty()));

    Var::new(ScopedHandle::global(handle))
  }
}

#[derive(Debug)]
pub(crate) enum ShaderDecl {
  Main(ErasedFun),
  FunDef(u16, ErasedFun),
  Const(u16, Type, ErasedExpr),
  In(u16, Type),
  Out(u16, Type),
}

macro_rules! make_vn {
  ($t:ident, $dim:expr) => {
    #[derive(Clone, Debug, PartialEq)]
    pub struct $t<T>([T; $dim]);

    impl<T> From<[T; $dim]> for $t<T> {
      fn from(a: [T; $dim]) -> Self {
        Self(a)
      }
    }
  };
}

make_vn!(V2, 2);
make_vn!(V3, 3);
make_vn!(V4, 4);

#[derive(Clone, Debug, PartialEq)]
pub enum ErasedExpr {
  // scalars
  LitInt(i32),
  LitUInt(u32),
  LitFloat(f32),
  LitBool(bool),
  // vectors
  LitInt2([i32; 2]),
  LitUInt2([u32; 2]),
  LitFloat2([f32; 2]),
  LitBool2([bool; 2]),
  LitInt3([i32; 3]),
  LitUInt3([u32; 3]),
  LitFloat3([f32; 3]),
  LitBool3([bool; 3]),
  LitInt4([i32; 4]),
  LitUInt4([u32; 4]),
  LitFloat4([f32; 4]),
  LitBool4([bool; 4]),
  // arrays
  Array(Type, Vec<ErasedExpr>),
  // var
  MutVar(ScopedHandle),
  ImmutBuiltIn(BuiltIn),
  // built-in functions and operators
  Not(Box<Self>),
  And(Box<Self>, Box<Self>),
  Or(Box<Self>, Box<Self>),
  Xor(Box<Self>, Box<Self>),
  BitOr(Box<Self>, Box<Self>),
  BitAnd(Box<Self>, Box<Self>),
  BitXor(Box<Self>, Box<Self>),
  Neg(Box<Self>),
  Add(Box<Self>, Box<Self>),
  Sub(Box<Self>, Box<Self>),
  Mul(Box<Self>, Box<Self>),
  Div(Box<Self>, Box<Self>),
  Rem(Box<Self>, Box<Self>),
  Shl(Box<Self>, Box<Self>),
  Shr(Box<Self>, Box<Self>),
  Eq(Box<Self>, Box<Self>),
  Neq(Box<Self>, Box<Self>),
  Lt(Box<Self>, Box<Self>),
  Lte(Box<Self>, Box<Self>),
  Gt(Box<Self>, Box<Self>),
  Gte(Box<Self>, Box<Self>),
  // function call
  FunCall(ErasedFunHandle, Vec<Self>),
  // swizzle
  Swizzle(Box<Self>, Swizzle),
  // field expression, as in a struct Foo { float x; }, foo.x is an Expr representing the x field on object foo
  Field { object: Box<Self>, field: Box<Self> },
  ArrayLookup { object: Box<Self>, index: Box<Self> },
}

#[derive(Debug)]
pub struct Expr<T>
where
  T: ?Sized,
{
  erased: ErasedExpr,
  _phantom: PhantomData<T>,
}

impl<T> From<&'_ Self> for Expr<T>
where
  T: ?Sized,
{
  fn from(e: &Self) -> Self {
    Self::new(e.erased.clone())
  }
}

impl<T> Clone for Expr<T>
where
  T: ?Sized,
{
  fn clone(&self) -> Self {
    Self::new(self.erased.clone())
  }
}

impl<T> Expr<T>
where
  T: ?Sized,
{
  const fn new(erased: ErasedExpr) -> Self {
    Self {
      erased,
      _phantom: PhantomData,
    }
  }

  const fn new_builtin(builtin: BuiltIn) -> Self {
    Self::new(ErasedExpr::MutVar(ScopedHandle::builtin(builtin)))
  }

  const fn new_immut_builtin(builtin: BuiltIn) -> Self {
    Self::new(ErasedExpr::ImmutBuiltIn(builtin))
  }

  pub fn eq(&self, rhs: impl Into<Expr<T>>) -> Expr<bool> {
    Expr::new(ErasedExpr::Eq(
      Box::new(self.erased.clone()),
      Box::new(rhs.into().erased),
    ))
  }

  pub fn neq(&self, rhs: impl Into<Expr<T>>) -> Expr<bool> {
    Expr::new(ErasedExpr::Neq(
      Box::new(self.erased.clone()),
      Box::new(rhs.into().erased),
    ))
  }
}

impl<T> Expr<T>
where
  T: PartialOrd,
{
  pub fn lt(&self, rhs: impl Into<Expr<T>>) -> Expr<bool> {
    Expr::new(ErasedExpr::Lt(
      Box::new(self.erased.clone()),
      Box::new(rhs.into().erased),
    ))
  }

  pub fn lte(&self, rhs: impl Into<Expr<T>>) -> Expr<bool> {
    Expr::new(ErasedExpr::Lte(
      Box::new(self.erased.clone()),
      Box::new(rhs.into().erased),
    ))
  }

  pub fn gt(&self, rhs: impl Into<Expr<T>>) -> Expr<bool> {
    Expr::new(ErasedExpr::Gt(
      Box::new(self.erased.clone()),
      Box::new(rhs.into().erased),
    ))
  }

  pub fn gte(&self, rhs: impl Into<Expr<T>>) -> Expr<bool> {
    Expr::new(ErasedExpr::Gte(
      Box::new(self.erased.clone()),
      Box::new(rhs.into().erased),
    ))
  }
}

impl Expr<bool> {
  pub fn and(&self, rhs: impl Into<Expr<bool>>) -> Expr<bool> {
    Expr::new(ErasedExpr::And(
      Box::new(self.erased.clone()),
      Box::new(rhs.into().erased),
    ))
  }

  pub fn or(&self, rhs: impl Into<Expr<bool>>) -> Expr<bool> {
    Expr::new(ErasedExpr::Or(
      Box::new(self.erased.clone()),
      Box::new(rhs.into().erased),
    ))
  }

  pub fn xor(&self, rhs: impl Into<Expr<bool>>) -> Expr<bool> {
    Expr::new(ErasedExpr::Xor(
      Box::new(self.erased.clone()),
      Box::new(rhs.into().erased),
    ))
  }
}

impl<T> Expr<[T]> {
  pub fn at(&self, index: impl Into<Expr<i32>>) -> Expr<T> {
    Expr::new(ErasedExpr::ArrayLookup {
      object: Box::new(self.erased.clone()),
      index: Box::new(index.into().erased),
    })
  }
}

impl<T, const N: usize> Expr<[T; N]> {
  pub fn at(&self, index: impl Into<Expr<i32>>) -> Expr<T> {
    Expr::new(ErasedExpr::ArrayLookup {
      object: Box::new(self.erased.clone()),
      index: Box::new(index.into().erased),
    })
  }
}

// not
macro_rules! impl_Not_Expr {
  ($t:ty) => {
    impl ops::Not for Expr<$t> {
      type Output = Self;

      fn not(self) -> Self::Output {
        Expr::new(ErasedExpr::Not(Box::new(self.erased)))
      }
    }

    impl<'a> ops::Not for &'a Expr<$t> {
      type Output = Expr<$t>;

      fn not(self) -> Self::Output {
        Expr::new(ErasedExpr::Not(Box::new(self.erased.clone())))
      }
    }
  };
}

impl_Not_Expr!(bool);
impl_Not_Expr!(V2<bool>);
impl_Not_Expr!(V3<bool>);
impl_Not_Expr!(V4<bool>);

// neg
macro_rules! impl_Neg_Expr {
  ($t:ty) => {
    impl ops::Neg for Expr<$t> {
      type Output = Self;

      fn neg(self) -> Self::Output {
        Expr::new(ErasedExpr::Neg(Box::new(self.erased)))
      }
    }

    impl<'a> ops::Neg for &'a Expr<$t> {
      type Output = Expr<$t>;

      fn neg(self) -> Self::Output {
        Expr::new(ErasedExpr::Neg(Box::new(self.erased.clone())))
      }
    }
  };
}

impl_Neg_Expr!(i32);
impl_Neg_Expr!(V2<i32>);
impl_Neg_Expr!(V3<i32>);
impl_Neg_Expr!(V4<i32>);

impl_Neg_Expr!(u32);
impl_Neg_Expr!(V2<u32>);
impl_Neg_Expr!(V3<u32>);
impl_Neg_Expr!(V4<u32>);

impl_Neg_Expr!(f32);
impl_Neg_Expr!(V2<f32>);
impl_Neg_Expr!(V3<f32>);
impl_Neg_Expr!(V4<f32>);

// binary arithmetic and logical (+, -, *, /, %)
// binop
macro_rules! impl_binop_Expr {
  ($op:ident, $meth_name:ident, $a:ty, $b:ty) => {
    // expr OP expr
    impl<'a> ops::$op<Expr<$b>> for Expr<$a> {
      type Output = Expr<$a>;

      fn $meth_name(self, rhs: Expr<$b>) -> Self::Output {
        Expr::new(ErasedExpr::$op(Box::new(self.erased), Box::new(rhs.erased)))
      }
    }

    impl<'a> ops::$op<&'a Expr<$b>> for Expr<$a> {
      type Output = Expr<$a>;

      fn $meth_name(self, rhs: &'a Expr<$b>) -> Self::Output {
        Expr::new(ErasedExpr::$op(
          Box::new(self.erased),
          Box::new(rhs.erased.clone()),
        ))
      }
    }

    impl<'a> ops::$op<Expr<$b>> for &'a Expr<$a> {
      type Output = Expr<$a>;

      fn $meth_name(self, rhs: Expr<$b>) -> Self::Output {
        Expr::new(ErasedExpr::$op(
          Box::new(self.erased.clone()),
          Box::new(rhs.erased),
        ))
      }
    }

    impl<'a> ops::$op<&'a Expr<$b>> for &'a Expr<$a> {
      type Output = Expr<$a>;

      fn $meth_name(self, rhs: &'a Expr<$b>) -> Self::Output {
        Expr::new(ErasedExpr::$op(
          Box::new(self.erased.clone()),
          Box::new(rhs.erased.clone()),
        ))
      }
    }

    // expr OP t, where t is automatically lifted
    impl<'a> ops::$op<$b> for Expr<$a> {
      type Output = Expr<$a>;

      fn $meth_name(self, rhs: $b) -> Self::Output {
        let rhs = Expr::from(rhs);
        Expr::new(ErasedExpr::$op(Box::new(self.erased), Box::new(rhs.erased)))
      }
    }

    impl<'a> ops::$op<$b> for &'a Expr<$a> {
      type Output = Expr<$a>;

      fn $meth_name(self, rhs: $b) -> Self::Output {
        let rhs: Expr<$b> = rhs.into();
        Expr::new(ErasedExpr::$op(
          Box::new(self.erased.clone()),
          Box::new(rhs.erased),
        ))
      }
    }
  };
}

// or
impl_binop_Expr!(BitOr, bitor, bool, bool);
impl_binop_Expr!(BitOr, bitor, V2<bool>, V2<bool>);
impl_binop_Expr!(BitOr, bitor, V2<bool>, bool);
impl_binop_Expr!(BitOr, bitor, V3<bool>, V3<bool>);
impl_binop_Expr!(BitOr, bitor, V3<bool>, bool);
impl_binop_Expr!(BitOr, bitor, V4<bool>, V4<bool>);
impl_binop_Expr!(BitOr, bitor, V4<bool>, bool);

// and
impl_binop_Expr!(BitAnd, bitand, bool, bool);
impl_binop_Expr!(BitAnd, bitand, V2<bool>, V2<bool>);
impl_binop_Expr!(BitAnd, bitand, V2<bool>, bool);
impl_binop_Expr!(BitAnd, bitand, V3<bool>, V3<bool>);
impl_binop_Expr!(BitAnd, bitand, V3<bool>, bool);
impl_binop_Expr!(BitAnd, bitand, V4<bool>, V4<bool>);
impl_binop_Expr!(BitAnd, bitand, V4<bool>, bool);

// xor
impl_binop_Expr!(BitXor, bitxor, bool, bool);
impl_binop_Expr!(BitXor, bitxor, V2<bool>, V2<bool>);
impl_binop_Expr!(BitXor, bitxor, V2<bool>, bool);
impl_binop_Expr!(BitXor, bitxor, V3<bool>, V3<bool>);
impl_binop_Expr!(BitXor, bitxor, V3<bool>, bool);
impl_binop_Expr!(BitXor, bitxor, V4<bool>, V4<bool>);
impl_binop_Expr!(BitXor, bitxor, V4<bool>, bool);

/// Run a macro on all supported types to generate the impl for them
///
/// The macro has to have to take two `ty` as argument and yield a `std::ops` trait implementor.
macro_rules! impl_binarith_Expr {
  ($op:ident, $meth_name:ident) => {
    impl_binop_Expr!($op, $meth_name, i32, i32);
    impl_binop_Expr!($op, $meth_name, V2<i32>, V2<i32>);
    impl_binop_Expr!($op, $meth_name, V2<i32>, i32);
    impl_binop_Expr!($op, $meth_name, V3<i32>, V3<i32>);
    impl_binop_Expr!($op, $meth_name, V3<i32>, i32);
    impl_binop_Expr!($op, $meth_name, V4<i32>, V4<i32>);
    impl_binop_Expr!($op, $meth_name, V4<i32>, i32);

    impl_binop_Expr!($op, $meth_name, u32, u32);
    impl_binop_Expr!($op, $meth_name, V2<u32>, V2<u32>);
    impl_binop_Expr!($op, $meth_name, V2<u32>, u32);
    impl_binop_Expr!($op, $meth_name, V3<u32>, V3<u32>);
    impl_binop_Expr!($op, $meth_name, V3<u32>, u32);
    impl_binop_Expr!($op, $meth_name, V4<u32>, V4<u32>);
    impl_binop_Expr!($op, $meth_name, V4<u32>, u32);

    impl_binop_Expr!($op, $meth_name, f32, f32);
    impl_binop_Expr!($op, $meth_name, V2<f32>, V2<f32>);
    impl_binop_Expr!($op, $meth_name, V2<f32>, f32);
    impl_binop_Expr!($op, $meth_name, V3<f32>, V3<f32>);
    impl_binop_Expr!($op, $meth_name, V3<f32>, f32);
    impl_binop_Expr!($op, $meth_name, V4<f32>, V4<f32>);
    impl_binop_Expr!($op, $meth_name, V4<f32>, f32);
  };
}

impl_binarith_Expr!(Add, add);
impl_binarith_Expr!(Sub, sub);
impl_binarith_Expr!(Mul, mul);
impl_binarith_Expr!(Div, div);

impl_binop_Expr!(Rem, rem, f32, f32);
impl_binop_Expr!(Rem, rem, V2<f32>, V2<f32>);
impl_binop_Expr!(Rem, rem, V2<f32>, f32);
impl_binop_Expr!(Rem, rem, V3<f32>, V3<f32>);
impl_binop_Expr!(Rem, rem, V3<f32>, f32);
impl_binop_Expr!(Rem, rem, V4<f32>, V4<f32>);
impl_binop_Expr!(Rem, rem, V4<f32>, f32);

macro_rules! impl_binshift_Expr {
  ($op:ident, $meth_name:ident, $ty:ty) => {
    // expr OP expr
    impl ops::$op<Expr<u32>> for Expr<$ty> {
      type Output = Expr<$ty>;

      fn $meth_name(self, rhs: Expr<u32>) -> Self::Output {
        Expr::new(ErasedExpr::$op(Box::new(self.erased), Box::new(rhs.erased)))
      }
    }

    impl<'a> ops::$op<Expr<u32>> for &'a Expr<$ty> {
      type Output = Expr<$ty>;

      fn $meth_name(self, rhs: Expr<u32>) -> Self::Output {
        Expr::new(ErasedExpr::$op(
          Box::new(self.erased.clone()),
          Box::new(rhs.erased),
        ))
      }
    }

    impl<'a> ops::$op<&'a Expr<u32>> for Expr<$ty> {
      type Output = Expr<$ty>;

      fn $meth_name(self, rhs: &'a Expr<u32>) -> Self::Output {
        Expr::new(ErasedExpr::$op(
          Box::new(self.erased),
          Box::new(rhs.erased.clone()),
        ))
      }
    }

    impl<'a> ops::$op<&'a Expr<u32>> for &'a Expr<$ty> {
      type Output = Expr<$ty>;

      fn $meth_name(self, rhs: &'a Expr<u32>) -> Self::Output {
        Expr::new(ErasedExpr::$op(
          Box::new(self.erased.clone()),
          Box::new(rhs.erased.clone()),
        ))
      }
    }

    // expr OP bits
    impl ops::$op<u32> for Expr<$ty> {
      type Output = Self;

      fn $meth_name(self, rhs: u32) -> Self::Output {
        let rhs = Expr::from(rhs);
        Expr::new(ErasedExpr::$op(Box::new(self.erased), Box::new(rhs.erased)))
      }
    }

    impl<'a> ops::$op<u32> for &'a Expr<$ty> {
      type Output = Expr<$ty>;

      fn $meth_name(self, rhs: u32) -> Self::Output {
        let rhs = Expr::from(rhs);
        Expr::new(ErasedExpr::$op(
          Box::new(self.erased.clone()),
          Box::new(rhs.erased),
        ))
      }
    }
  };
}

/// Binary shift generating macro.
macro_rules! impl_binshifts_Expr {
  ($op:ident, $meth_name:ident) => {
    impl_binshift_Expr!($op, $meth_name, i32);
    impl_binshift_Expr!($op, $meth_name, V2<i32>);
    impl_binshift_Expr!($op, $meth_name, V3<i32>);
    impl_binshift_Expr!($op, $meth_name, V4<i32>);

    impl_binshift_Expr!($op, $meth_name, u32);
    impl_binshift_Expr!($op, $meth_name, V2<u32>);
    impl_binshift_Expr!($op, $meth_name, V3<u32>);
    impl_binshift_Expr!($op, $meth_name, V4<u32>);

    impl_binshift_Expr!($op, $meth_name, f32);
    impl_binshift_Expr!($op, $meth_name, V2<f32>);
    impl_binshift_Expr!($op, $meth_name, V3<f32>);
    impl_binshift_Expr!($op, $meth_name, V4<f32>);
  };
}

impl_binshifts_Expr!(Shl, shl);
impl_binshifts_Expr!(Shr, shr);

macro_rules! impl_From_Expr_scalar {
  ($t:ty, $q:ident) => {
    impl From<$t> for Expr<$t> {
      fn from(a: $t) -> Self {
        Self::new(ErasedExpr::$q(a))
      }
    }

    impl<'a> From<&'a $t> for Expr<$t> {
      fn from(a: &'a $t) -> Self {
        Self::new(ErasedExpr::$q(*a))
      }
    }
  };
}

impl_From_Expr_scalar!(i32, LitInt);
impl_From_Expr_scalar!(u32, LitUInt);
impl_From_Expr_scalar!(f32, LitFloat);
impl_From_Expr_scalar!(bool, LitBool);

macro_rules! impl_From_Expr_vn {
  ($t:ty, $q:ident) => {
    impl From<$t> for Expr<$t> {
      fn from(a: $t) -> Self {
        Self::new(ErasedExpr::$q(a.0))
      }
    }

    impl<'a> From<&'a $t> for Expr<$t> {
      fn from(a: &'a $t) -> Self {
        Self::new(ErasedExpr::$q(a.0))
      }
    }
  };
}

impl_From_Expr_vn!(V2<i32>, LitInt2);
impl_From_Expr_vn!(V2<u32>, LitUInt2);
impl_From_Expr_vn!(V2<f32>, LitFloat2);
impl_From_Expr_vn!(V2<bool>, LitBool2);
impl_From_Expr_vn!(V3<i32>, LitInt3);
impl_From_Expr_vn!(V3<u32>, LitUInt3);
impl_From_Expr_vn!(V3<f32>, LitFloat3);
impl_From_Expr_vn!(V3<bool>, LitBool3);
impl_From_Expr_vn!(V4<i32>, LitInt4);
impl_From_Expr_vn!(V4<u32>, LitUInt4);
impl_From_Expr_vn!(V4<f32>, LitFloat4);
impl_From_Expr_vn!(V4<bool>, LitBool4);

impl<T, const N: usize> From<[T; N]> for Expr<[T; N]>
where
  Expr<T>: From<T>,
  T: Clone + ToType,
{
  fn from(array: [T; N]) -> Self {
    let array = array
      .iter()
      .cloned()
      .map(|t| Expr::from(t).erased)
      .collect();
    Self::new(ErasedExpr::Array(<[T; N] as ToType>::ty(), array))
  }
}

impl<'a, T, const N: usize> From<&'a [T; N]> for Expr<[T; N]>
where
  Expr<T>: From<T>,
  T: Clone + ToType,
{
  fn from(array: &'a [T; N]) -> Self {
    let array = array
      .iter()
      .cloned()
      .map(|t| Expr::from(t).erased)
      .collect();
    Self::new(ErasedExpr::Array(<[T; N] as ToType>::ty(), array))
  }
}

/// Easily create literal expressions.
///
/// TODO
#[macro_export]
macro_rules! lit {
  ($e:expr) => {
    $crate::Expr::from($e)
  };

  ($a:expr, $b:expr) => {
    $crate::Expr::from(V2::from([$a, $b]))
  };

  ($a:expr, $b:expr, $c:expr) => {
    $crate::Expr::from($crate::V3::from([$a, $b, $c]))
  };

  ($a:expr, $b:expr, $c:expr, $d:expr) => {
    $crate::Expr::from($crate::V4::from([$a, $b, $c, $d]))
  };
}

#[derive(Clone, Debug, PartialEq)]
pub enum ErasedReturn {
  Void,
  Expr(Type, ErasedExpr),
}

impl From<()> for ErasedReturn {
  fn from(_: ()) -> Self {
    ErasedReturn::Void
  }
}

impl<T> From<Expr<T>> for ErasedReturn
where
  T: ToType,
{
  fn from(expr: Expr<T>) -> Self {
    ErasedReturn::Expr(T::ty(), expr.erased)
  }
}

pub trait ToFun<R, A> {
  fn build_fn(self) -> FunDef<R, A>;
}

impl<F, R> ToFun<R, ()> for F
where
  Self: Fn(&mut Scope<R>) -> R,
  ErasedReturn: From<R>,
{
  fn build_fn(self) -> FunDef<R, ()> {
    let mut scope = Scope::new(0);
    let ret = self(&mut scope);

    let erased = ErasedFun::new(Vec::new(), scope.erased, ErasedReturn::from(ret));

    FunDef::new(erased)
  }
}

macro_rules! impl_ToFun_args {
  ($($arg:ident , $arg_ident:ident , $arg_rank:expr),*) => {
    impl<F, R, $($arg),*> ToFun<R, ($(Expr<$arg>),*)> for F
    where
      Self: Fn(&mut Scope<R>, $(Expr<$arg>),*) -> R,
      ErasedReturn: From<R>,
      $($arg: ToType),*
    {
      fn build_fn(self) -> FunDef<R, ($(Expr<$arg>),*)> {
        $( let $arg_ident = Expr::new(ErasedExpr::MutVar(ScopedHandle::fun_arg($arg_rank))); )*
          let args = vec![$( $arg::ty() ),*];

        let mut scope = Scope::new(0);
        let ret = self(&mut scope, $($arg_ident),*);

        let erased = ErasedFun::new(args, scope.erased, ErasedReturn::from(ret));

        FunDef::new(erased)
      }
    }
  }
}

impl<F, R, A> ToFun<R, Expr<A>> for F
where
  Self: Fn(&mut Scope<R>, Expr<A>) -> R,
  ErasedReturn: From<R>,
  A: ToType,
{
  fn build_fn(self) -> FunDef<R, Expr<A>> {
    let arg = Expr::new(ErasedExpr::MutVar(ScopedHandle::fun_arg(0)));

    let mut scope = Scope::new(0);
    let ret = self(&mut scope, arg);

    let erased = ErasedFun::new(vec![A::ty()], scope.erased, ErasedReturn::from(ret));

    FunDef::new(erased)
  }
}

impl_ToFun_args!(A0, a0, 0, A1, a1, 1);
impl_ToFun_args!(A0, a0, 0, A1, a1, 1, A2, a2, 2);
impl_ToFun_args!(A0, a0, 0, A1, a1, 1, A2, a2, 2, A3, a3, 3);
impl_ToFun_args!(A0, a0, 0, A1, a1, 1, A2, a2, 2, A3, a3, 3, A4, a4, 4);
impl_ToFun_args!(A0, a0, 0, A1, a1, 1, A2, a2, 2, A3, a3, 3, A4, a4, 4, A5, a5, 5);
impl_ToFun_args!(A0, a0, 0, A1, a1, 1, A2, a2, 2, A3, a3, 3, A4, a4, 4, A5, a5, 5, A6, a6, 6);
impl_ToFun_args!(
  A0, a0, 0, A1, a1, 1, A2, a2, 2, A3, a3, 3, A4, a4, 4, A5, a5, 5, A6, a6, 6, A7, a7, 7
);
impl_ToFun_args!(
  A0, a0, 0, A1, a1, 1, A2, a2, 2, A3, a3, 3, A4, a4, 4, A5, a5, 5, A6, a6, 6, A7, a7, 7, A8, a8, 8
);
impl_ToFun_args!(
  A0, a0, 0, A1, a1, 1, A2, a2, 2, A3, a3, 3, A4, a4, 4, A5, a5, 5, A6, a6, 6, A7, a7, 7, A8, a8,
  8, A9, a9, 9
);
impl_ToFun_args!(
  A0, a0, 0, A1, a1, 1, A2, a2, 2, A3, a3, 3, A4, a4, 4, A5, a5, 5, A6, a6, 6, A7, a7, 7, A8, a8,
  8, A9, a9, 9, A10, a10, 10
);
impl_ToFun_args!(
  A0, a0, 0, A1, a1, 1, A2, a2, 2, A3, a3, 3, A4, a4, 4, A5, a5, 5, A6, a6, 6, A7, a7, 7, A8, a8,
  8, A9, a9, 9, A10, a10, 10, A11, a11, 11
);
impl_ToFun_args!(
  A0, a0, 0, A1, a1, 1, A2, a2, 2, A3, a3, 3, A4, a4, 4, A5, a5, 5, A6, a6, 6, A7, a7, 7, A8, a8,
  8, A9, a9, 9, A10, a10, 10, A11, a11, 11, A12, a12, 12
);
impl_ToFun_args!(
  A0, a0, 0, A1, a1, 1, A2, a2, 2, A3, a3, 3, A4, a4, 4, A5, a5, 5, A6, a6, 6, A7, a7, 7, A8, a8,
  8, A9, a9, 9, A10, a10, 10, A11, a11, 11, A12, a12, 12, A13, a13, 13
);
impl_ToFun_args!(
  A0, a0, 0, A1, a1, 1, A2, a2, 2, A3, a3, 3, A4, a4, 4, A5, a5, 5, A6, a6, 6, A7, a7, 7, A8, a8,
  8, A9, a9, 9, A10, a10, 10, A11, a11, 11, A12, a12, 12, A13, a13, 13, A14, a14, 14
);
impl_ToFun_args!(
  A0, a0, 0, A1, a1, 1, A2, a2, 2, A3, a3, 3, A4, a4, 4, A5, a5, 5, A6, a6, 6, A7, a7, 7, A8, a8,
  8, A9, a9, 9, A10, a10, 10, A11, a11, 11, A12, a12, 12, A13, a13, 13, A14, a14, 14, A15, a15, 15
);
impl_ToFun_args!(
  A0, a0, 0, A1, a1, 1, A2, a2, 2, A3, a3, 3, A4, a4, 4, A5, a5, 5, A6, a6, 6, A7, a7, 7, A8, a8,
  8, A9, a9, 9, A10, a10, 10, A11, a11, 11, A12, a12, 12, A13, a13, 13, A14, a14, 14, A15, a15, 15,
  A16, a16, 16
);

#[derive(Clone, Debug, PartialEq)]
pub struct FunHandle<R, A> {
  erased: ErasedFunHandle,
  _phantom: PhantomData<(R, A)>,
}

impl<R> FunHandle<Expr<R>, ()> {
  pub fn call(&self) -> Expr<R> {
    Expr::new(ErasedExpr::FunCall(self.erased.clone(), Vec::new()))
  }
}

#[cfg(feature = "fun-call")]
impl<R> FnOnce<()> for FunHandle<Expr<R>, ()> {
  type Output = Expr<R>;

  extern "rust-call" fn call_once(self, _: ()) -> Self::Output {
    self.call()
  }
}

#[cfg(feature = "fun-call")]
impl<R> FnMut<()> for FunHandle<Expr<R>, ()> {
  extern "rust-call" fn call_mut(&mut self, _: ()) -> Self::Output {
    self.call()
  }
}

#[cfg(feature = "fun-call")]
impl<R> Fn<()> for FunHandle<Expr<R>, ()> {
  extern "rust-call" fn call(&self, _: ()) -> Self::Output {
    self.call()
  }
}

impl<R, A> FunHandle<Expr<R>, Expr<A>> {
  pub fn call(&self, a: Expr<A>) -> Expr<R> {
    Expr::new(ErasedExpr::FunCall(self.erased.clone(), vec![a.erased]))
  }
}

#[cfg(feature = "fun-call")]
impl<R, A> FnOnce<(Expr<A>,)> for FunHandle<Expr<R>, Expr<A>> {
  type Output = Expr<R>;

  extern "rust-call" fn call_once(self, a: (Expr<A>,)) -> Self::Output {
    self.call(a.0)
  }
}

#[cfg(feature = "fun-call")]
impl<R, A> FnMut<(Expr<A>,)> for FunHandle<Expr<R>, Expr<A>> {
  extern "rust-call" fn call_mut(&mut self, a: (Expr<A>,)) -> Self::Output {
    self.call(a.0)
  }
}

#[cfg(feature = "fun-call")]
impl<R, A> Fn<(Expr<A>,)> for FunHandle<Expr<R>, Expr<A>> {
  extern "rust-call" fn call(&self, a: (Expr<A>,)) -> Self::Output {
    self.call(a.0)
  }
}

// the first stage must be named S0
macro_rules! impl_FunCall {
  ( $( ( $arg_name:ident, $arg_ty:ident ) ),*) => {
    impl<R, $($arg_ty),*> FunHandle<Expr<R>, ($(Expr<$arg_ty>),*)>
    {
      pub fn call(&self, $($arg_name : Expr<$arg_ty>),*) -> Expr<R> {
        Expr::new(ErasedExpr::FunCall(self.erased.clone(), vec![$($arg_name.erased),*]))
      }
    }

    #[cfg(feature = "fun-call")]
    impl<R, $($arg_ty),*> FnOnce<($(Expr<$arg_ty>),*)> for FunHandle<Expr<R>, ($(Expr<$arg_ty>),*)>
    {
      type Output = Expr<R>;

      extern "rust-call" fn call_once(self, ($($arg_name),*): ($(Expr<$arg_ty>),*)) -> Self::Output {
        self.call($($arg_name),*)
      }
    }

    #[cfg(feature = "fun-call")]
    impl<R, $($arg_ty),*> FnMut<($(Expr<$arg_ty>),*)> for FunHandle<Expr<R>, ($(Expr<$arg_ty>),*)>
    {
      extern "rust-call" fn call_mut(&mut self, ($($arg_name),*): ($(Expr<$arg_ty>),*)) -> Self::Output {
        self.call($($arg_name),*)
      }
    }

    #[cfg(feature = "fun-call")]
    impl<R, $($arg_ty),*> Fn<($(Expr<$arg_ty>),*)> for FunHandle<Expr<R>, ($(Expr<$arg_ty>),*)>
    {
      extern "rust-call" fn call(&self, ($($arg_name),*): ($(Expr<$arg_ty>),*)) -> Self::Output {
        self.call($($arg_name),*)
      }
    }
  };
}

// implement function calls for Expr up to 16 arguments
macro_rules! impl_FunCall_rec {
  ( ( $a:ident, $b:ident ) , ( $x:ident, $y:ident )) => {
    impl_FunCall!(($a, $b), ($x, $y));
  };

  ( ( $a:ident, $b:ident ) , ( $x: ident, $y: ident ) , $($r:tt)* ) => {
    impl_FunCall_rec!(($a, $b), $($r)*);
    impl_FunCall!(($a, $b), ($x, $y), $($r)*);
  };
}
impl_FunCall_rec!(
  (a, A),
  (b, B),
  (c, C),
  (d, D),
  (e, E),
  (f, F),
  (g, G),
  (h, H),
  (i, I),
  (j, J),
  (k, K),
  (l, L),
  (m, M),
  (n, N),
  (o, O),
  (p, P)
);

#[derive(Clone, Debug, PartialEq)]
pub enum ErasedFunHandle {
  Main,
  // trigonometry
  Radians,
  Degrees,
  Sin,
  Cos,
  Tan,
  ASin,
  ACos,
  ATan,
  SinH,
  CosH,
  TanH,
  ASinH,
  ACosH,
  ATanH,
  // exponential
  Pow,
  Exp,
  Exp2,
  Log,
  Log2,
  Sqrt,
  InverseSqrt,
  // common
  Abs,
  Sign,
  Floor,
  Trunc,
  Round,
  RoundEven,
  Ceil,
  Fract,
  Min,
  Max,
  Clamp,
  Mix,
  Step,
  SmoothStep,
  IsNan,
  IsInf,
  FloatBitsToInt,
  IntBitsToFloat,
  UIntBitsToFloat,
  FMA,
  Frexp,
  Ldexp,
  // floating-point pack and unpack functions
  PackUnorm2x16,
  PackSnorm2x16,
  PackUnorm4x8,
  PackSnorm4x8,
  UnpackUnorm2x16,
  UnpackSnorm2x16,
  UnpackUnorm4x8,
  UnpackSnorm4x8,
  PackHalf2x16,
  UnpackHalf2x16,
  // geometry functions
  Length,
  Distance,
  Dot,
  Cross,
  Normalize,
  FaceForward,
  Reflect,
  Refract,
  // matrix functions
  // TODO
  // vector relational functions
  VLt,
  VLte,
  VGt,
  VGte,
  VEq,
  VNeq,
  VAny,
  VAll,
  VNot,
  // integer functions
  UAddCarry,
  USubBorrow,
  UMulExtended,
  IMulExtended,
  BitfieldExtract,
  BitfieldInsert,
  BitfieldReverse,
  BitCount,
  FindLSB,
  FindMSB,
  // texture functions
  // TODO
  // geometry shader functions
  EmitStreamVertex,
  EndStreamPrimitive,
  EmitVertex,
  EndPrimitive,
  // fragment processing functions
  DFDX,
  DFDY,
  DFDXFine,
  DFDYFine,
  DFDXCoarse,
  DFDYCoarse,
  FWidth,
  FWidthFine,
  FWidthCoarse,
  InterpolateAtCentroid,
  InterpolateAtSample,
  InterpolateAtOffset,
  // shader invocation control functions
  Barrier,
  MemoryBarrier,
  MemoryBarrierAtomic,
  MemoryBarrierBuffer,
  MemoryBarrierShared,
  MemoryBarrierImage,
  GroupMemoryBarrier,
  // shader invocation group functions
  AnyInvocation,
  AllInvocations,
  AllInvocationsEqual,
  UserDefined(u16),
}

#[derive(Clone, Debug)]
pub struct FunDef<R, A> {
  erased: ErasedFun,
  _phantom: PhantomData<(R, A)>,
}

impl<R, A> FunDef<R, A> {
  fn new(erased: ErasedFun) -> Self {
    Self {
      erased,
      _phantom: PhantomData,
    }
  }
}

#[derive(Clone, Debug)]
pub struct ErasedFun {
  args: Vec<Type>,
  scope: ErasedScope,
  ret: ErasedReturn,
}

impl ErasedFun {
  fn new(args: Vec<Type>, scope: ErasedScope, ret: ErasedReturn) -> Self {
    Self { args, scope, ret }
  }
}

#[derive(Clone, Debug)]
pub struct Scope<R> {
  erased: ErasedScope,
  _phantom: PhantomData<R>,
}

impl<R> Scope<R>
where
  ErasedReturn: From<R>,
{
  fn new(id: u16) -> Self {
    Self {
      erased: ErasedScope::new(id),
      _phantom: PhantomData,
    }
  }

  fn deeper(&self) -> Self {
    Scope::new(self.erased.id + 1)
  }

  pub fn var<T>(&mut self, init_value: impl Into<Expr<T>>) -> Var<T>
  where
    T: ToType,
  {
    let n = self.erased.next_var;
    let handle = ScopedHandle::fun_var(self.erased.id, n);

    self.erased.next_var += 1;

    self.erased.instructions.push(ScopeInstr::VarDecl {
      ty: T::ty(),
      handle,
      init_value: init_value.into().erased,
    });

    Var::new(handle)
  }

  pub fn leave(&mut self, ret: impl Into<R>) {
    self
      .erased
      .instructions
      .push(ScopeInstr::Return(ErasedReturn::from(ret.into())));
  }

  pub fn abort(&mut self) {
    self
      .erased
      .instructions
      .push(ScopeInstr::Return(ErasedReturn::Void));
  }

  pub fn when<'a>(
    &'a mut self,
    condition: impl Into<Expr<bool>>,
    body: impl Fn(&mut Scope<R>),
  ) -> When<'a, R> {
    let mut scope = self.deeper();
    body(&mut scope);

    self.erased.instructions.push(ScopeInstr::If {
      condition: condition.into().erased,
      scope: scope.erased,
    });

    When { parent_scope: self }
  }

  pub fn unless<'a>(
    &'a mut self,
    condition: impl Into<Expr<bool>>,
    body: impl Fn(&mut Scope<R>),
  ) -> When<'a, R> {
    self.when(!condition.into(), body)
  }

  pub fn loop_for<T>(
    &mut self,
    init_value: impl Into<Expr<T>>,
    condition: impl Fn(&Expr<T>) -> Expr<bool>,
    iter_fold: impl Fn(&Expr<T>) -> Expr<T>,
    body: impl Fn(&mut Scope<R>, &Expr<T>),
  ) where
    T: ToType,
  {
    let mut scope = self.deeper();

    // bind the init value so that it’s available in all closures
    let init_var = scope.var(init_value);

    let condition = condition(&init_var);

    // generate the “post expr”, which is basically the free from of the third part of the for loop; people usually
    // set this to ++i, i++, etc., but in our case, the expression is to treat as a fold’s accumulator
    let post_expr = iter_fold(&init_var);

    body(&mut scope, &init_var);

    self.erased.instructions.push(ScopeInstr::For {
      init_ty: T::ty(),
      init_handle: ScopedHandle::fun_var(scope.erased.id, 0),
      init_expr: init_var.to_expr().erased,
      condition: condition.erased,
      post_expr: post_expr.erased,
      scope: scope.erased,
    });
  }

  pub fn loop_while(&mut self, condition: impl Into<Expr<bool>>, body: impl Fn(&mut Scope<R>)) {
    let mut scope = self.deeper();
    body(&mut scope);

    self.erased.instructions.push(ScopeInstr::While {
      condition: condition.into().erased,
      scope: scope.erased,
    });
  }

  pub fn loop_continue(&mut self) {
    self.erased.instructions.push(ScopeInstr::Continue);
  }

  pub fn loop_break(&mut self) {
    self.erased.instructions.push(ScopeInstr::Break);
  }

  pub fn set<T>(&mut self, var: impl Into<Var<T>>, value: impl Into<Expr<T>>) {
    self.erased.instructions.push(ScopeInstr::MutateVar {
      var: var.into().to_expr().erased,
      expr: value.into().erased,
    });
  }
}

#[derive(Clone, Debug, PartialEq)]
struct ErasedScope {
  id: u16,
  instructions: Vec<ScopeInstr>,
  next_var: u16,
}

impl ErasedScope {
  fn new(id: u16) -> Self {
    Self {
      id,
      instructions: Vec::new(),
      next_var: 0,
    }
  }
}

pub struct When<'a, R> {
  /// The scope from which this [`When`] expression comes from.
  ///
  /// This will be handy if we want to chain this when with others (corresponding to `else if` and `else`, for
  /// instance).
  parent_scope: &'a mut Scope<R>,
}

impl<R> When<'_, R>
where
  ErasedReturn: From<R>,
{
  pub fn or_else(self, condition: impl Into<Expr<bool>>, body: impl Fn(&mut Scope<R>)) -> Self {
    let mut scope = self.parent_scope.deeper();
    body(&mut scope);

    self
      .parent_scope
      .erased
      .instructions
      .push(ScopeInstr::ElseIf {
        condition: condition.into().erased,
        scope: scope.erased,
      });

    self
  }

  pub fn or(self, body: impl Fn(&mut Scope<R>)) {
    let mut scope = self.parent_scope.deeper();
    body(&mut scope);

    self
      .parent_scope
      .erased
      .instructions
      .push(ScopeInstr::Else {
        scope: scope.erased,
      });
  }
}

#[derive(Debug)]
pub struct Var<T>(Expr<T>)
where
  T: ?Sized;

impl<'a, T> From<&'a Var<T>> for Var<T>
where
  T: ?Sized,
{
  fn from(v: &'a Self) -> Self {
    Var(v.0.clone())
  }
}

impl<T> From<Var<T>> for Expr<T>
where
  T: ?Sized,
{
  fn from(v: Var<T>) -> Self {
    v.0
  }
}

impl<'a, T> From<&'a Var<T>> for Expr<T>
where
  T: ?Sized,
{
  fn from(v: &'a Var<T>) -> Self {
    v.0.clone()
  }
}

impl<T> Var<T>
where
  T: ?Sized,
{
  pub const fn new(handle: ScopedHandle) -> Self {
    Self(Expr::new(ErasedExpr::MutVar(handle)))
  }

  pub fn to_expr(&self) -> Expr<T> {
    self.0.clone()
  }
}

impl<T> Var<[T]> {
  pub fn at(&self, index: impl Into<Expr<i32>>) -> Var<T> {
    Var(self.to_expr().at(index))
  }
}

impl<T> ops::Deref for Var<T>
where
  T: ?Sized,
{
  type Target = Expr<T>;

  fn deref(&self) -> &Self::Target {
    &self.0
  }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ScopedHandle {
  BuiltIn(BuiltIn),
  Global(u16),
  FunArg(u16),
  FunVar { subscope: u16, handle: u16 },
}

impl ScopedHandle {
  const fn builtin(b: BuiltIn) -> Self {
    Self::BuiltIn(b)
  }

  const fn global(handle: u16) -> Self {
    Self::Global(handle)
  }

  const fn fun_arg(handle: u16) -> Self {
    Self::FunArg(handle)
  }

  const fn fun_var(subscope: u16, handle: u16) -> Self {
    Self::FunVar { subscope, handle }
  }
}

#[derive(Clone, Debug, PartialEq)]
enum ScopeInstr {
  VarDecl {
    ty: Type,
    handle: ScopedHandle,
    init_value: ErasedExpr,
  },

  Return(ErasedReturn),

  Continue,

  Break,

  If {
    condition: ErasedExpr,
    scope: ErasedScope,
  },

  ElseIf {
    condition: ErasedExpr,
    scope: ErasedScope,
  },

  Else {
    scope: ErasedScope,
  },

  For {
    init_ty: Type,
    init_handle: ScopedHandle,
    init_expr: ErasedExpr,
    condition: ErasedExpr,
    post_expr: ErasedExpr,
    scope: ErasedScope,
  },

  While {
    condition: ErasedExpr,
    scope: ErasedScope,
  },

  MutateVar {
    var: ErasedExpr,
    expr: ErasedExpr,
  },
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum Dim {
  Scalar,
  D2,
  D3,
  D4,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Type {
  prim_ty: PrimType,
  /// Array dimensions, if any.
  ///
  /// Dimensions are sorted from outer to inner; i.e. `[[i32; N]; M]`’s dimensions is encoded as `vec![M, N]`.
  array_dims: Vec<usize>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum PrimType {
  Int(Dim),
  UInt(Dim),
  Float(Dim),
  Bool(Dim),
}

pub trait ToPrimType {
  const PRIM_TYPE: PrimType;
}

macro_rules! impl_ToPrimType {
  ($t:ty, $q:ident, $d:ident) => {
    impl ToPrimType for $t {
      const PRIM_TYPE: PrimType = PrimType::$q(Dim::$d);
    }
  };
}

impl_ToPrimType!(i32, Int, Scalar);
impl_ToPrimType!(u32, UInt, Scalar);
impl_ToPrimType!(f32, Float, Scalar);
impl_ToPrimType!(bool, Bool, Scalar);
impl_ToPrimType!(V2<i32>, Int, D2);
impl_ToPrimType!(V2<u32>, UInt, D2);
impl_ToPrimType!(V2<f32>, Float, D2);
impl_ToPrimType!(V2<bool>, Bool, D2);
impl_ToPrimType!(V3<i32>, Int, D3);
impl_ToPrimType!(V3<u32>, UInt, D3);
impl_ToPrimType!(V3<f32>, Float, D3);
impl_ToPrimType!(V3<bool>, Bool, D3);
impl_ToPrimType!(V4<i32>, Int, D4);
impl_ToPrimType!(V4<u32>, UInt, D4);
impl_ToPrimType!(V4<f32>, Float, D4);
impl_ToPrimType!(V4<bool>, Bool, D4);

pub trait ToType {
  fn ty() -> Type;
}

impl<T> ToType for T
where
  T: ToPrimType,
{
  fn ty() -> Type {
    Type {
      prim_ty: T::PRIM_TYPE,
      array_dims: Vec::new(),
    }
  }
}

impl<T, const N: usize> ToType for [T; N]
where
  T: ToType,
{
  fn ty() -> Type {
    let Type {
      prim_ty,
      array_dims,
    } = T::ty();
    let array_dims = once(N).chain(array_dims).collect();

    Type {
      prim_ty,
      array_dims,
    }
  }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SwizzleSelector {
  X,
  Y,
  Z,
  W,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Swizzle {
  D1(SwizzleSelector),
  D2(SwizzleSelector, SwizzleSelector),
  D3(SwizzleSelector, SwizzleSelector, SwizzleSelector),
  D4(
    SwizzleSelector,
    SwizzleSelector,
    SwizzleSelector,
    SwizzleSelector,
  ),
}

pub trait Swizzlable<S> {
  fn swizzle(&self, sw: S) -> Self;
}

// 2D
impl<T> Swizzlable<SwizzleSelector> for Expr<V2<T>> {
  fn swizzle(&self, x: SwizzleSelector) -> Self {
    Expr::new(ErasedExpr::Swizzle(
      Box::new(self.erased.clone()),
      Swizzle::D1(x),
    ))
  }
}

impl<T> Swizzlable<[SwizzleSelector; 2]> for Expr<V2<T>> {
  fn swizzle(&self, [x, y]: [SwizzleSelector; 2]) -> Self {
    Expr::new(ErasedExpr::Swizzle(
      Box::new(self.erased.clone()),
      Swizzle::D2(x, y),
    ))
  }
}

// 3D
impl<T> Swizzlable<SwizzleSelector> for Expr<V3<T>> {
  fn swizzle(&self, x: SwizzleSelector) -> Self {
    Expr::new(ErasedExpr::Swizzle(
      Box::new(self.erased.clone()),
      Swizzle::D1(x),
    ))
  }
}

impl<T> Swizzlable<[SwizzleSelector; 2]> for Expr<V3<T>> {
  fn swizzle(&self, [x, y]: [SwizzleSelector; 2]) -> Self {
    Expr::new(ErasedExpr::Swizzle(
      Box::new(self.erased.clone()),
      Swizzle::D2(x, y),
    ))
  }
}

impl<T> Swizzlable<[SwizzleSelector; 3]> for Expr<V3<T>> {
  fn swizzle(&self, [x, y, z]: [SwizzleSelector; 3]) -> Self {
    Expr::new(ErasedExpr::Swizzle(
      Box::new(self.erased.clone()),
      Swizzle::D3(x, y, z),
    ))
  }
}

// 4D
impl<T> Swizzlable<SwizzleSelector> for Expr<V4<T>> {
  fn swizzle(&self, x: SwizzleSelector) -> Self {
    Expr::new(ErasedExpr::Swizzle(
      Box::new(self.erased.clone()),
      Swizzle::D1(x),
    ))
  }
}

impl<T> Swizzlable<[SwizzleSelector; 2]> for Expr<V4<T>> {
  fn swizzle(&self, [x, y]: [SwizzleSelector; 2]) -> Self {
    Expr::new(ErasedExpr::Swizzle(
      Box::new(self.erased.clone()),
      Swizzle::D2(x, y),
    ))
  }
}

impl<T> Swizzlable<[SwizzleSelector; 3]> for Expr<V4<T>> {
  fn swizzle(&self, [x, y, z]: [SwizzleSelector; 3]) -> Self {
    Expr::new(ErasedExpr::Swizzle(
      Box::new(self.erased.clone()),
      Swizzle::D3(x, y, z),
    ))
  }
}

impl<T> Swizzlable<[SwizzleSelector; 4]> for Expr<V4<T>> {
  fn swizzle(&self, [x, y, z, w]: [SwizzleSelector; 4]) -> Self {
    Expr::new(ErasedExpr::Swizzle(
      Box::new(self.erased.clone()),
      Swizzle::D4(x, y, z, w),
    ))
  }
}

#[macro_export]
macro_rules! sw {
  ($e:expr, . $a:tt) => {
    $e.swizzle(sw_extract!($a))
  };

  ($e:expr, . $a:tt . $b:tt) => {
    $e.swizzle([sw_extract!($a), sw_extract!($b)])
  };

  ($e:expr, . $a:tt . $b:tt . $c:tt) => {
    $e.swizzle([sw_extract!($a), sw_extract!($b), sw_extract!($c)])
  };

  ($e:expr, . $a:tt . $b:tt . $c:tt . $d:tt) => {
    $e.swizzle([
      sw_extract!($a),
      sw_extract!($b),
      sw_extract!($c),
      sw_extract!($d),
    ])
  };
}

#[macro_export]
macro_rules! sw_extract {
  (x) => {
    SwizzleSelector::X
  };

  (r) => {
    SwizzleSelector::X
  };

  (y) => {
    SwizzleSelector::Y
  };

  (g) => {
    SwizzleSelector::Y
  };

  (z) => {
    SwizzleSelector::Z
  };

  (b) => {
    SwizzleSelector::Z
  };

  (w) => {
    SwizzleSelector::Z
  };

  (a) => {
    SwizzleSelector::Z
  };
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum BuiltIn {
  Vertex(VertexBuiltIn),
  TessCtrl(TessCtrlBuiltIn),
  TessEval(TessEvalBuiltIn),
  Geometry(GeometryBuiltIn),
  Fragment(FragmentBuiltIn),
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum VertexBuiltIn {
  VertexID,
  InstanceID,
  BaseVertex,
  BaseInstance,
  Position,
  PointSize,
  ClipDistance,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum TessCtrlBuiltIn {
  MaxPatchVerticesIn,
  PatchVerticesIn,
  PrimitiveID,
  InvocationID,
  TessellationLevelOuter,
  TessellationLevelInner,
  In,
  Out,
  Position,
  PointSize,
  ClipDistance,
  CullDistance,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum TessEvalBuiltIn {
  TessCoord,
  MaxPatchVerticesIn,
  PatchVerticesIn,
  PrimitiveID,
  TessellationLevelOuter,
  TessellationLevelInner,
  In,
  Out,
  Position,
  PointSize,
  ClipDistance,
  CullDistance,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum GeometryBuiltIn {
  In,
  Out,
  Position,
  PointSize,
  ClipDistance,
  CullDistance,
  PrimitiveID,
  PrimitiveIDIn,
  InvocationID,
  Layer,
  ViewportIndex,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum FragmentBuiltIn {
  FragCoord,
  FrontFacing,
  PointCoord,
  SampleID,
  SamplePosition,
  SampleMaskIn,
  ClipDistance,
  CullDistance,
  PrimitiveID,
  Layer,
  ViewportIndex,
  FragDepth,
  SampleMask,
  HelperInvocation,
}

// vertex shader built-ins
pub struct VertexShaderEnv {
  // inputs
  pub vertex_id: Expr<i32>,
  pub instance_id: Expr<i32>,
  pub base_vertex: Expr<i32>,
  pub base_instance: Expr<i32>,
  // outputs
  pub position: Var<V4<f32>>,
  pub point_size: Var<f32>,
  pub clip_distance: Var<[f32]>,
}

impl VertexShaderEnv {
  fn new() -> Self {
    let vertex_id = Expr::new_immut_builtin(BuiltIn::Vertex(VertexBuiltIn::VertexID));
    let instance_id = Expr::new_immut_builtin(BuiltIn::Vertex(VertexBuiltIn::InstanceID));
    let base_vertex = Expr::new_immut_builtin(BuiltIn::Vertex(VertexBuiltIn::BaseVertex));
    let base_instance = Expr::new_immut_builtin(BuiltIn::Vertex(VertexBuiltIn::BaseInstance));
    let position = Var(Expr::new_builtin(BuiltIn::Vertex(VertexBuiltIn::Position)));
    let point_size = Var(Expr::new_builtin(BuiltIn::Vertex(VertexBuiltIn::PointSize)));
    let clip_distance = Var(Expr::new_builtin(BuiltIn::Vertex(
      VertexBuiltIn::ClipDistance,
    )));

    Self {
      vertex_id,
      instance_id,
      base_vertex,
      base_instance,
      position,
      point_size,
      clip_distance,
    }
  }
}

// tessellation control shader built-ins
pub struct TessCtrlShaderEnv {
  // inputs
  pub max_patch_vertices_in: Expr<i32>,
  pub patch_vertices_in: Expr<i32>,
  pub primitive_id: Expr<i32>,
  pub invocation_id: Expr<i32>,
  pub input: Expr<[TessControlPerVertexIn]>,
  // outputs
  pub tess_level_outer: Var<[f32; 4]>,
  pub tess_level_inner: Var<[f32; 2]>,
  pub output: Var<[TessControlPerVertexOut]>,
}

impl TessCtrlShaderEnv {
  fn new() -> Self {
    let max_patch_vertices_in =
      Expr::new_immut_builtin(BuiltIn::TessCtrl(TessCtrlBuiltIn::MaxPatchVerticesIn));
    let patch_vertices_in =
      Expr::new_immut_builtin(BuiltIn::TessCtrl(TessCtrlBuiltIn::PatchVerticesIn));
    let primitive_id = Expr::new_immut_builtin(BuiltIn::TessCtrl(TessCtrlBuiltIn::PrimitiveID));
    let invocation_id = Expr::new_immut_builtin(BuiltIn::TessCtrl(TessCtrlBuiltIn::InvocationID));
    let input = Expr::new_immut_builtin(BuiltIn::TessCtrl(TessCtrlBuiltIn::In));
    let tess_level_outer = Var::new(ScopedHandle::BuiltIn(BuiltIn::TessCtrl(
      TessCtrlBuiltIn::TessellationLevelOuter,
    )));
    let tess_level_inner = Var::new(ScopedHandle::BuiltIn(BuiltIn::TessCtrl(
      TessCtrlBuiltIn::TessellationLevelInner,
    )));
    let output = Var::new(ScopedHandle::BuiltIn(BuiltIn::TessCtrl(
      TessCtrlBuiltIn::Out,
    )));

    Self {
      max_patch_vertices_in,
      patch_vertices_in,
      primitive_id,
      invocation_id,
      input,
      tess_level_outer,
      tess_level_inner,
      output,
    }
  }
}

pub struct TessControlPerVertexIn;

impl Expr<TessControlPerVertexIn> {
  pub fn position(&self) -> Expr<V4<f32>> {
    let erased = ErasedExpr::Field {
      object: Box::new(self.erased.clone()),
      field: Box::new(ErasedExpr::ImmutBuiltIn(BuiltIn::TessCtrl(
        TessCtrlBuiltIn::Position,
      ))),
    };

    Expr::new(erased)
  }

  pub fn point_size(&self) -> Expr<f32> {
    let erased = ErasedExpr::Field {
      object: Box::new(self.erased.clone()),
      field: Box::new(ErasedExpr::ImmutBuiltIn(BuiltIn::TessCtrl(
        TessCtrlBuiltIn::PointSize,
      ))),
    };

    Expr::new(erased)
  }

  pub fn clip_distance(&self) -> Expr<[f32]> {
    let erased = ErasedExpr::Field {
      object: Box::new(self.erased.clone()),
      field: Box::new(ErasedExpr::ImmutBuiltIn(BuiltIn::TessCtrl(
        TessCtrlBuiltIn::ClipDistance,
      ))),
    };

    Expr::new(erased)
  }

  pub fn cull_distance(&self) -> Expr<[f32]> {
    let erased = ErasedExpr::Field {
      object: Box::new(self.erased.clone()),
      field: Box::new(ErasedExpr::ImmutBuiltIn(BuiltIn::TessCtrl(
        TessCtrlBuiltIn::CullDistance,
      ))),
    };

    Expr::new(erased)
  }
}

pub struct TessControlPerVertexOut(());

impl Expr<TessControlPerVertexOut> {
  pub fn position(&self) -> Var<V4<f32>> {
    let expr = ErasedExpr::Field {
      object: Box::new(self.erased.clone()),
      field: Box::new(ErasedExpr::ImmutBuiltIn(BuiltIn::TessCtrl(
        TessCtrlBuiltIn::Position,
      ))),
    };

    Var(Expr::new(expr))
  }

  pub fn point_size(&self) -> Var<f32> {
    let expr = ErasedExpr::Field {
      object: Box::new(self.erased.clone()),
      field: Box::new(ErasedExpr::ImmutBuiltIn(BuiltIn::TessCtrl(
        TessCtrlBuiltIn::PointSize,
      ))),
    };

    Var(Expr::new(expr))
  }

  pub fn clip_distance(&self) -> Var<[f32]> {
    let expr = ErasedExpr::Field {
      object: Box::new(self.erased.clone()),
      field: Box::new(ErasedExpr::ImmutBuiltIn(BuiltIn::TessCtrl(
        TessCtrlBuiltIn::ClipDistance,
      ))),
    };

    Var(Expr::new(expr))
  }

  pub fn cull_distance(&self) -> Var<[f32]> {
    let expr = ErasedExpr::Field {
      object: Box::new(self.erased.clone()),
      field: Box::new(ErasedExpr::ImmutBuiltIn(BuiltIn::TessCtrl(
        TessCtrlBuiltIn::CullDistance,
      ))),
    };

    Var(Expr::new(expr))
  }
}

// tessellation evalution shader built-ins; inputs
pub struct TessEvalShaderEnv {
  // inputs
  pub patch_vertices_in: Expr<i32>,
  pub primitive_id: Expr<i32>,
  pub tess_coord: Expr<V3<f32>>,
  pub tess_level_outer: Expr<[f32; 4]>,
  pub tess_level_inner: Expr<[f32; 2]>,
  pub input: Expr<[TessEvaluationPerVertexIn]>,
  // outputs
  pub position: Var<V4<f32>>,
  pub point_size: Var<f32>,
  pub clip_distance: Var<[f32]>,
  pub cull_distance: Var<[f32]>,
}

impl TessEvalShaderEnv {
  fn new() -> Self {
    let patch_vertices_in =
      Expr::new_immut_builtin(BuiltIn::TessEval(TessEvalBuiltIn::PatchVerticesIn));
    let primitive_id = Expr::new_immut_builtin(BuiltIn::TessEval(TessEvalBuiltIn::PrimitiveID));
    let tess_coord = Expr::new_immut_builtin(BuiltIn::TessEval(TessEvalBuiltIn::TessCoord));
    let tess_level_outer =
      Expr::new_immut_builtin(BuiltIn::TessEval(TessEvalBuiltIn::TessellationLevelOuter));
    let tess_level_inner =
      Expr::new_immut_builtin(BuiltIn::TessEval(TessEvalBuiltIn::TessellationLevelInner));
    let input = Expr::new_immut_builtin(BuiltIn::TessEval(TessEvalBuiltIn::In));

    let position = Var::new(ScopedHandle::BuiltIn(BuiltIn::TessEval(
      TessEvalBuiltIn::Position,
    )));
    let point_size = Var::new(ScopedHandle::BuiltIn(BuiltIn::TessEval(
      TessEvalBuiltIn::PointSize,
    )));
    let clip_distance = Var::new(ScopedHandle::BuiltIn(BuiltIn::TessEval(
      TessEvalBuiltIn::ClipDistance,
    )));
    let cull_distance = Var::new(ScopedHandle::BuiltIn(BuiltIn::TessEval(
      TessEvalBuiltIn::ClipDistance,
    )));

    Self {
      patch_vertices_in,
      primitive_id,
      tess_coord,
      tess_level_outer,
      tess_level_inner,
      input,
      position,
      point_size,
      clip_distance,
      cull_distance,
    }
  }
}

pub struct TessEvaluationPerVertexIn;

impl Expr<TessEvaluationPerVertexIn> {
  pub fn position(&self) -> Expr<V4<f32>> {
    let erased = ErasedExpr::Field {
      object: Box::new(self.erased.clone()),
      field: Box::new(ErasedExpr::ImmutBuiltIn(BuiltIn::TessEval(
        TessEvalBuiltIn::Position,
      ))),
    };

    Expr::new(erased)
  }

  pub fn point_size(&self) -> Expr<f32> {
    let erased = ErasedExpr::Field {
      object: Box::new(self.erased.clone()),
      field: Box::new(ErasedExpr::ImmutBuiltIn(BuiltIn::TessEval(
        TessEvalBuiltIn::PointSize,
      ))),
    };

    Expr::new(erased)
  }

  pub fn clip_distance(&self) -> Expr<[f32]> {
    let erased = ErasedExpr::Field {
      object: Box::new(self.erased.clone()),
      field: Box::new(ErasedExpr::ImmutBuiltIn(BuiltIn::TessEval(
        TessEvalBuiltIn::ClipDistance,
      ))),
    };

    Expr::new(erased)
  }

  pub fn cull_distance(&self) -> Expr<[f32]> {
    let erased = ErasedExpr::Field {
      object: Box::new(self.erased.clone()),
      field: Box::new(ErasedExpr::ImmutBuiltIn(BuiltIn::TessEval(
        TessEvalBuiltIn::CullDistance,
      ))),
    };

    Expr::new(erased)
  }
}

// geometry shader built-ins; inputs
pub struct GeometryShaderEnv {
  // inputs
  pub primitive_id_in: Expr<i32>,
  pub invocation_id: Expr<i32>,
  pub input: Expr<[GeometryPerVertexIn]>,
  // outputs
  pub position: Var<V4<f32>>,
  pub point_size: Var<f32>,
  pub clip_distance: Var<[f32]>,
  pub cull_distance: Var<[f32]>,
  pub primitive_id: Var<i32>,
  pub layer: Var<i32>,
  pub viewport_index: Var<i32>,
}

impl GeometryShaderEnv {
  fn new() -> Self {
    let primitive_id_in =
      Expr::new_immut_builtin(BuiltIn::Geometry(GeometryBuiltIn::PrimitiveIDIn));
    let invocation_id = Expr::new_immut_builtin(BuiltIn::Geometry(GeometryBuiltIn::InvocationID));
    let input = Expr::new_immut_builtin(BuiltIn::Geometry(GeometryBuiltIn::In));

    let position = Var::new(ScopedHandle::BuiltIn(BuiltIn::Geometry(
      GeometryBuiltIn::Position,
    )));
    let point_size = Var::new(ScopedHandle::BuiltIn(BuiltIn::Geometry(
      GeometryBuiltIn::PointSize,
    )));
    let clip_distance = Var::new(ScopedHandle::BuiltIn(BuiltIn::Geometry(
      GeometryBuiltIn::ClipDistance,
    )));
    let cull_distance = Var::new(ScopedHandle::BuiltIn(BuiltIn::Geometry(
      GeometryBuiltIn::CullDistance,
    )));
    let primitive_id = Var::new(ScopedHandle::BuiltIn(BuiltIn::Geometry(
      GeometryBuiltIn::PrimitiveID,
    )));
    let layer = Var::new(ScopedHandle::BuiltIn(BuiltIn::Geometry(
      GeometryBuiltIn::Layer,
    )));
    let viewport_index = Var::new(ScopedHandle::BuiltIn(BuiltIn::Geometry(
      GeometryBuiltIn::ViewportIndex,
    )));

    Self {
      primitive_id_in,
      invocation_id,
      input,
      position,
      point_size,
      clip_distance,
      cull_distance,
      primitive_id,
      layer,
      viewport_index,
    }
  }
}

pub struct GeometryPerVertexIn;

impl Expr<GeometryPerVertexIn> {
  pub fn position(&self) -> Expr<V4<f32>> {
    let erased = ErasedExpr::Field {
      object: Box::new(self.erased.clone()),
      field: Box::new(ErasedExpr::ImmutBuiltIn(BuiltIn::Geometry(
        GeometryBuiltIn::Position,
      ))),
    };

    Expr::new(erased)
  }

  pub fn point_size(&self) -> Expr<f32> {
    let erased = ErasedExpr::Field {
      object: Box::new(self.erased.clone()),
      field: Box::new(ErasedExpr::ImmutBuiltIn(BuiltIn::Geometry(
        GeometryBuiltIn::PointSize,
      ))),
    };

    Expr::new(erased)
  }

  pub fn clip_distance(&self) -> Expr<[f32]> {
    let erased = ErasedExpr::Field {
      object: Box::new(self.erased.clone()),
      field: Box::new(ErasedExpr::ImmutBuiltIn(BuiltIn::Geometry(
        GeometryBuiltIn::ClipDistance,
      ))),
    };

    Expr::new(erased)
  }

  pub fn cull_distance(&self) -> Expr<[f32]> {
    let erased = ErasedExpr::Field {
      object: Box::new(self.erased.clone()),
      field: Box::new(ErasedExpr::ImmutBuiltIn(BuiltIn::Geometry(
        GeometryBuiltIn::CullDistance,
      ))),
    };

    Expr::new(erased)
  }
}

// fragment shader built-ins
pub struct FragmentShaderEnv {
  // inputs
  pub frag_coord: Expr<V4<f32>>,
  pub front_facing: Expr<bool>,
  pub clip_distance: Expr<[f32]>,
  pub cull_distance: Expr<[f32]>,
  pub point_coord: Expr<V2<f32>>,
  pub primitive_id: Expr<i32>,
  pub sample_id: Expr<i32>,
  pub sample_position: Expr<V2<f32>>,
  pub sample_mask_in: Expr<i32>,
  pub layer: Expr<i32>,
  pub viewport_index: Expr<i32>,
  pub helper_invocation: Expr<bool>,
  // outputs
  pub frag_depth: Var<f32>,
  pub sample_mask: Var<[i32]>,
}

impl FragmentShaderEnv {
  fn new() -> Self {
    let frag_coord = Expr::new_builtin(BuiltIn::Fragment(FragmentBuiltIn::FragCoord));
    let front_facing = Expr::new_builtin(BuiltIn::Fragment(FragmentBuiltIn::FrontFacing));
    let clip_distance = Expr::new_builtin(BuiltIn::Fragment(FragmentBuiltIn::ClipDistance));
    let cull_distance = Expr::new_builtin(BuiltIn::Fragment(FragmentBuiltIn::CullDistance));
    let point_coord = Expr::new_builtin(BuiltIn::Fragment(FragmentBuiltIn::PointCoord));
    let primitive_id = Expr::new_builtin(BuiltIn::Fragment(FragmentBuiltIn::PrimitiveID));
    let sample_id = Expr::new_builtin(BuiltIn::Fragment(FragmentBuiltIn::SampleID));
    let sample_position = Expr::new_builtin(BuiltIn::Fragment(FragmentBuiltIn::SamplePosition));
    let sample_mask_in = Expr::new_builtin(BuiltIn::Fragment(FragmentBuiltIn::SampleMaskIn));
    let layer = Expr::new_builtin(BuiltIn::Fragment(FragmentBuiltIn::Layer));
    let viewport_index = Expr::new_builtin(BuiltIn::Fragment(FragmentBuiltIn::ViewportIndex));
    let helper_invocation = Expr::new_builtin(BuiltIn::Fragment(FragmentBuiltIn::HelperInvocation));

    let frag_depth = Var::new(ScopedHandle::BuiltIn(BuiltIn::Fragment(
      FragmentBuiltIn::FragDepth,
    )));
    let sample_mask = Var::new(ScopedHandle::BuiltIn(BuiltIn::Fragment(
      FragmentBuiltIn::SampleMask,
    )));

    Self {
      frag_coord,
      front_facing,
      clip_distance,
      cull_distance,
      point_coord,
      primitive_id,
      sample_id,
      sample_position,
      sample_mask_in,
      layer,
      viewport_index,
      helper_invocation,
      frag_depth,
      sample_mask,
    }
  }
}

// standard library

pub trait Trigonometry {
  fn radians(&self) -> Self;

  fn degrees(&self) -> Self;

  fn sin(&self) -> Self;

  fn cos(&self) -> Self;

  fn tan(&self) -> Self;

  fn asin(&self) -> Self;

  fn acos(&self) -> Self;

  fn atan(&self) -> Self;

  fn sinh(&self) -> Self;

  fn cosh(&self) -> Self;

  fn tanh(&self) -> Self;

  fn asinh(&self) -> Self;

  fn acosh(&self) -> Self;

  fn atanh(&self) -> Self;
}

macro_rules! impl_Trigonometry {
  ($t:ty) => {
    impl Trigonometry for Expr<$t> {
      fn radians(&self) -> Self {
        Expr::new(ErasedExpr::FunCall(
          ErasedFunHandle::Radians,
          vec![self.erased.clone()],
        ))
      }

      fn degrees(&self) -> Self {
        Expr::new(ErasedExpr::FunCall(
          ErasedFunHandle::Degrees,
          vec![self.erased.clone()],
        ))
      }

      fn sin(&self) -> Self {
        Expr::new(ErasedExpr::FunCall(
          ErasedFunHandle::Sin,
          vec![self.erased.clone()],
        ))
      }

      fn cos(&self) -> Self {
        Expr::new(ErasedExpr::FunCall(
          ErasedFunHandle::Cos,
          vec![self.erased.clone()],
        ))
      }

      fn tan(&self) -> Self {
        Expr::new(ErasedExpr::FunCall(
          ErasedFunHandle::Tan,
          vec![self.erased.clone()],
        ))
      }

      fn asin(&self) -> Self {
        Expr::new(ErasedExpr::FunCall(
          ErasedFunHandle::ASin,
          vec![self.erased.clone()],
        ))
      }

      fn acos(&self) -> Self {
        Expr::new(ErasedExpr::FunCall(
          ErasedFunHandle::ACos,
          vec![self.erased.clone()],
        ))
      }

      fn atan(&self) -> Self {
        Expr::new(ErasedExpr::FunCall(
          ErasedFunHandle::ATan,
          vec![self.erased.clone()],
        ))
      }

      fn sinh(&self) -> Self {
        Expr::new(ErasedExpr::FunCall(
          ErasedFunHandle::SinH,
          vec![self.erased.clone()],
        ))
      }

      fn cosh(&self) -> Self {
        Expr::new(ErasedExpr::FunCall(
          ErasedFunHandle::CosH,
          vec![self.erased.clone()],
        ))
      }

      fn tanh(&self) -> Self {
        Expr::new(ErasedExpr::FunCall(
          ErasedFunHandle::TanH,
          vec![self.erased.clone()],
        ))
      }

      fn asinh(&self) -> Self {
        Expr::new(ErasedExpr::FunCall(
          ErasedFunHandle::ASinH,
          vec![self.erased.clone()],
        ))
      }

      fn acosh(&self) -> Self {
        Expr::new(ErasedExpr::FunCall(
          ErasedFunHandle::ACosH,
          vec![self.erased.clone()],
        ))
      }

      fn atanh(&self) -> Self {
        Expr::new(ErasedExpr::FunCall(
          ErasedFunHandle::ATanH,
          vec![self.erased.clone()],
        ))
      }
    }
  };
}

impl_Trigonometry!(f32);
impl_Trigonometry!(V2<f32>);
impl_Trigonometry!(V3<f32>);
impl_Trigonometry!(V4<f32>);

pub trait Exponential: Sized {
  fn pow(&self, p: impl Into<Self>) -> Self;

  fn exp(&self) -> Self;

  fn exp2(&self) -> Self;

  fn log(&self) -> Self;

  fn log2(&self) -> Self;

  fn sqrt(&self) -> Self;

  fn isqrt(&self) -> Self;
}

macro_rules! impl_Exponential {
  ($t:ty) => {
    impl Exponential for Expr<$t> {
      fn pow(&self, p: impl Into<Self>) -> Self {
        Expr::new(ErasedExpr::FunCall(
          ErasedFunHandle::Pow,
          vec![self.erased.clone(), p.into().erased],
        ))
      }

      fn exp(&self) -> Self {
        Expr::new(ErasedExpr::FunCall(
          ErasedFunHandle::Exp,
          vec![self.erased.clone()],
        ))
      }

      fn exp2(&self) -> Self {
        Expr::new(ErasedExpr::FunCall(
          ErasedFunHandle::Exp2,
          vec![self.erased.clone()],
        ))
      }

      fn log(&self) -> Self {
        Expr::new(ErasedExpr::FunCall(
          ErasedFunHandle::Log,
          vec![self.erased.clone()],
        ))
      }

      fn log2(&self) -> Self {
        Expr::new(ErasedExpr::FunCall(
          ErasedFunHandle::Log2,
          vec![self.erased.clone()],
        ))
      }

      fn sqrt(&self) -> Self {
        Expr::new(ErasedExpr::FunCall(
          ErasedFunHandle::Sqrt,
          vec![self.erased.clone()],
        ))
      }

      fn isqrt(&self) -> Self {
        Expr::new(ErasedExpr::FunCall(
          ErasedFunHandle::InverseSqrt,
          vec![self.erased.clone()],
        ))
      }
    }
  };
}

impl_Exponential!(f32);
impl_Exponential!(V2<f32>);
impl_Exponential!(V3<f32>);
impl_Exponential!(V4<f32>);

pub trait Relative {
  fn abs(&self) -> Self;

  fn sign(&self) -> Self;
}

macro_rules! impl_Relative {
  ($t:ty) => {
    impl Relative for Expr<$t> {
      fn abs(&self) -> Self {
        Expr::new(ErasedExpr::FunCall(
          ErasedFunHandle::Abs,
          vec![self.erased.clone()],
        ))
      }

      fn sign(&self) -> Self {
        Expr::new(ErasedExpr::FunCall(
          ErasedFunHandle::Sign,
          vec![self.erased.clone()],
        ))
      }
    }
  };
}

impl_Relative!(i32);
impl_Relative!(V2<i32>);
impl_Relative!(V3<i32>);
impl_Relative!(V4<i32>);
impl_Relative!(f32);
impl_Relative!(V2<f32>);
impl_Relative!(V3<f32>);
impl_Relative!(V4<f32>);

pub trait Floating {
  fn floor(&self) -> Self;

  fn trunc(&self) -> Self;

  fn round(&self) -> Self;

  fn ceil(&self) -> Self;

  fn fract(&self) -> Self;
}

macro_rules! impl_Floating {
  ($t:ty) => {
    impl Floating for Expr<$t> {
      fn floor(&self) -> Self {
        Expr::new(ErasedExpr::FunCall(
          ErasedFunHandle::Floor,
          vec![self.erased.clone()],
        ))
      }

      fn trunc(&self) -> Self {
        Expr::new(ErasedExpr::FunCall(
          ErasedFunHandle::Trunc,
          vec![self.erased.clone()],
        ))
      }

      fn round(&self) -> Self {
        Expr::new(ErasedExpr::FunCall(
          ErasedFunHandle::Round,
          vec![self.erased.clone()],
        ))
      }

      fn ceil(&self) -> Self {
        Expr::new(ErasedExpr::FunCall(
          ErasedFunHandle::Ceil,
          vec![self.erased.clone()],
        ))
      }

      fn fract(&self) -> Self {
        Expr::new(ErasedExpr::FunCall(
          ErasedFunHandle::Fract,
          vec![self.erased.clone()],
        ))
      }
    }
  };
}

impl_Floating!(f32);
impl_Floating!(V2<f32>);
impl_Floating!(V3<f32>);
impl_Floating!(V4<f32>);

trait Bounded: Sized {
  fn min(&self, rhs: impl Into<Self>) -> Self;

  fn max(&self, rhs: impl Into<Self>) -> Self;

  fn clamp(&self, min_value: impl Into<Self>, max_value: impl Into<Self>) -> Self;
}

macro_rules! impl_Bounded {
  ($t:ty) => {
    impl Bounded for Expr<$t> {
      fn min(&self, rhs: impl Into<Self>) -> Self {
        Expr::new(ErasedExpr::FunCall(
          ErasedFunHandle::Min,
          vec![self.erased.clone(), rhs.into().erased],
        ))
      }

      fn max(&self, rhs: impl Into<Self>) -> Self {
        Expr::new(ErasedExpr::FunCall(
          ErasedFunHandle::Max,
          vec![self.erased.clone(), rhs.into().erased],
        ))
      }

      fn clamp(&self, min_value: impl Into<Self>, max_value: impl Into<Self>) -> Self {
        Expr::new(ErasedExpr::FunCall(
          ErasedFunHandle::Clamp,
          vec![
            self.erased.clone(),
            min_value.into().erased,
            max_value.into().erased,
          ],
        ))
      }
    }
  };
}

impl_Bounded!(i32);
impl_Bounded!(V2<i32>);
impl_Bounded!(V3<i32>);
impl_Bounded!(V4<i32>);

impl_Bounded!(u32);
impl_Bounded!(V2<u32>);
impl_Bounded!(V3<u32>);
impl_Bounded!(V4<u32>);

impl_Bounded!(f32);
impl_Bounded!(V2<f32>);
impl_Bounded!(V3<f32>);
impl_Bounded!(V4<f32>);

impl_Bounded!(bool);
impl_Bounded!(V2<bool>);
impl_Bounded!(V3<bool>);
impl_Bounded!(V4<bool>);

pub trait Mix: Sized {
  fn mix(&self, y: impl Into<Self>, a: impl Into<Self>) -> Self;

  fn step(&self, edge: impl Into<Self>) -> Self;

  fn smooth_step(&self, edge_a: impl Into<Self>, edge_b: impl Into<Self>) -> Self;
}

macro_rules! impl_Mix {
  ($t:ty) => {
    impl Mix for Expr<$t> {
      fn mix(&self, y: impl Into<Self>, a: impl Into<Self>) -> Self {
        Expr::new(ErasedExpr::FunCall(
          ErasedFunHandle::Mix,
          vec![self.erased.clone(), y.into().erased, a.into().erased],
        ))
      }

      fn step(&self, edge: impl Into<Self>) -> Self {
        Expr::new(ErasedExpr::FunCall(
          ErasedFunHandle::Step,
          vec![self.erased.clone(), edge.into().erased],
        ))
      }

      fn smooth_step(&self, edge_a: impl Into<Self>, edge_b: impl Into<Self>) -> Self {
        Expr::new(ErasedExpr::FunCall(
          ErasedFunHandle::SmoothStep,
          vec![
            self.erased.clone(),
            edge_a.into().erased,
            edge_b.into().erased,
          ],
        ))
      }
    }
  };
}

impl_Mix!(f32);
impl_Mix!(V2<f32>);
impl_Mix!(V3<f32>);
impl_Mix!(V4<f32>);

pub trait FloatingExt {
  type BoolExpr;

  fn is_nan(&self) -> Self::BoolExpr;

  fn is_inf(&self) -> Self::BoolExpr;
}

macro_rules! impl_FloatingExt {
  ($t:ty, $bool_expr:ty) => {
    impl FloatingExt for Expr<$t> {
      type BoolExpr = Expr<$bool_expr>;

      fn is_nan(&self) -> Self::BoolExpr {
        Expr::new(ErasedExpr::FunCall(
          ErasedFunHandle::IsNan,
          vec![self.erased.clone()],
        ))
      }

      fn is_inf(&self) -> Self::BoolExpr {
        Expr::new(ErasedExpr::FunCall(
          ErasedFunHandle::IsInf,
          vec![self.erased.clone()],
        ))
      }
    }
  };
}

impl_FloatingExt!(f32, bool);
impl_FloatingExt!(V2<f32>, V2<bool>);
impl_FloatingExt!(V3<f32>, V3<bool>);
impl_FloatingExt!(V4<f32>, V4<bool>);

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn expr_lit() {
    assert_eq!(lit!(true).erased, ErasedExpr::LitBool(true));
    assert_eq!(lit![1, 2].erased, ErasedExpr::LitInt2([1, 2]));
  }

  #[test]
  fn expr_unary() {
    let mut scope = Scope::<()>::new(0);

    let a = !lit!(true);
    let b = -lit!(3i32);
    let c = scope.var(17);

    assert_eq!(
      a.erased,
      ErasedExpr::Not(Box::new(ErasedExpr::LitBool(true)))
    );
    assert_eq!(b.erased, ErasedExpr::Neg(Box::new(ErasedExpr::LitInt(3))));
    assert_eq!(c.erased, ErasedExpr::MutVar(ScopedHandle::fun_var(0, 0)));
  }

  #[test]
  fn expr_binary() {
    let a = lit!(1i32) + lit!(2);
    let b = lit!(1i32) + 2;

    assert_eq!(a.erased, b.erased);
    assert_eq!(
      a.erased,
      ErasedExpr::Add(
        Box::new(ErasedExpr::LitInt(1)),
        Box::new(ErasedExpr::LitInt(2)),
      )
    );
    assert_eq!(
      b.erased,
      ErasedExpr::Add(
        Box::new(ErasedExpr::LitInt(1)),
        Box::new(ErasedExpr::LitInt(2)),
      )
    );

    let a = lit!(1i32) - lit!(2);
    let b = lit!(1i32) - 2;

    assert_eq!(a.erased, b.erased);
    assert_eq!(
      a.erased,
      ErasedExpr::Sub(
        Box::new(ErasedExpr::LitInt(1)),
        Box::new(ErasedExpr::LitInt(2)),
      )
    );
    assert_eq!(
      b.erased,
      ErasedExpr::Sub(
        Box::new(ErasedExpr::LitInt(1)),
        Box::new(ErasedExpr::LitInt(2)),
      )
    );

    let a = lit!(1i32) * lit!(2);
    let b = lit!(1i32) * 2;

    assert_eq!(a.erased, b.erased);
    assert_eq!(
      a.erased,
      ErasedExpr::Mul(
        Box::new(ErasedExpr::LitInt(1)),
        Box::new(ErasedExpr::LitInt(2)),
      )
    );
    assert_eq!(
      b.erased,
      ErasedExpr::Mul(
        Box::new(ErasedExpr::LitInt(1)),
        Box::new(ErasedExpr::LitInt(2)),
      )
    );

    let a = lit!(1i32) / lit!(2);
    let b = lit!(1i32) / 2;

    assert_eq!(a.erased, b.erased);
    assert_eq!(
      a.erased,
      ErasedExpr::Div(
        Box::new(ErasedExpr::LitInt(1)),
        Box::new(ErasedExpr::LitInt(2)),
      )
    );
    assert_eq!(
      b.erased,
      ErasedExpr::Div(
        Box::new(ErasedExpr::LitInt(1)),
        Box::new(ErasedExpr::LitInt(2)),
      )
    );
  }

  #[test]
  fn expr_ref_inference() {
    let a = lit!(1i32);
    let b = a.clone() + 1;
    let c = a + 1;

    assert_eq!(b.erased, c.erased);
  }

  #[test]
  fn expr_var() {
    let mut scope = Scope::<()>::new(0);

    let x = scope.var(0);
    let y = scope.var(1u32);
    let z = scope.var(lit![false, true, false]);

    assert_eq!(x.erased, ErasedExpr::MutVar(ScopedHandle::fun_var(0, 0)));
    assert_eq!(y.erased, ErasedExpr::MutVar(ScopedHandle::fun_var(0, 1)));
    assert_eq!(
      z.erased,
      ErasedExpr::MutVar(ScopedHandle::fun_var(0, 2).into())
    );
    assert_eq!(scope.erased.instructions.len(), 3);
    assert_eq!(
      scope.erased.instructions[0],
      ScopeInstr::VarDecl {
        ty: Type {
          prim_ty: PrimType::Int(Dim::Scalar),
          array_dims: Vec::new(),
        },
        handle: ScopedHandle::fun_var(0, 0),
        init_value: ErasedExpr::LitInt(0),
      }
    );
    assert_eq!(
      scope.erased.instructions[1],
      ScopeInstr::VarDecl {
        ty: Type {
          prim_ty: PrimType::UInt(Dim::Scalar),
          array_dims: Vec::new(),
        },
        handle: ScopedHandle::fun_var(0, 1),
        init_value: ErasedExpr::LitUInt(1),
      }
    );
    assert_eq!(
      scope.erased.instructions[2],
      ScopeInstr::VarDecl {
        ty: Type {
          prim_ty: PrimType::Bool(Dim::D3),
          array_dims: Vec::new(),
        },
        handle: ScopedHandle::fun_var(0, 2),
        init_value: ErasedExpr::LitBool3([false, true, false]),
      }
    );
  }

  #[test]
  fn min_max_clamp() {
    let a = lit!(1i32);
    let b = lit!(2);
    let c = lit!(3);

    assert_eq!(
      a.min(&b).erased,
      ErasedExpr::FunCall(
        ErasedFunHandle::Min,
        vec![ErasedExpr::LitInt(1), ErasedExpr::LitInt(2)],
      )
    );

    assert_eq!(
      a.max(&b).erased,
      ErasedExpr::FunCall(
        ErasedFunHandle::Max,
        vec![ErasedExpr::LitInt(1), ErasedExpr::LitInt(2)],
      )
    );

    assert_eq!(
      a.clamp(b, c).erased,
      ErasedExpr::FunCall(
        ErasedFunHandle::Clamp,
        vec![
          ErasedExpr::LitInt(1),
          ErasedExpr::LitInt(2),
          ErasedExpr::LitInt(3)
        ],
      )
    );
  }

  #[test]
  fn fun0() {
    let mut shader = Shader::new();
    let fun = shader.fun(|s: &mut Scope<()>| {
      let _x = s.var(3);
    });

    assert_eq!(fun.erased, ErasedFunHandle::UserDefined(0));

    match shader.decls[0] {
      ShaderDecl::FunDef(0, ref fun) => {
        assert_eq!(fun.ret, ErasedReturn::Void);
        assert_eq!(fun.args, vec![]);
        assert_eq!(fun.scope.instructions.len(), 1);
        assert_eq!(
          fun.scope.instructions[0],
          ScopeInstr::VarDecl {
            ty: Type {
              prim_ty: PrimType::Int(Dim::Scalar),
              array_dims: Vec::new(),
            },
            handle: ScopedHandle::fun_var(0, 0),
            init_value: ErasedExpr::LitInt(3),
          }
        )
      }
      _ => panic!("wrong type"),
    }
  }

  #[test]
  fn fun1() {
    let mut shader = Shader::new();
    let fun = shader.fun(|f: &mut Scope<Expr<i32>>, _arg: Expr<i32>| {
      let x = f.var(lit!(3i32));
      x.into()
    });

    assert_eq!(fun.erased, ErasedFunHandle::UserDefined(0));

    match shader.decls[0] {
      ShaderDecl::FunDef(0, ref fun) => {
        assert_eq!(
          fun.ret,
          ErasedReturn::Expr(i32::ty(), ErasedExpr::MutVar(ScopedHandle::fun_var(0, 0)))
        );
        assert_eq!(
          fun.args,
          vec![Type {
            prim_ty: PrimType::Int(Dim::Scalar),
            array_dims: Vec::new(),
          }]
        );
        assert_eq!(fun.scope.instructions.len(), 1);
        assert_eq!(
          fun.scope.instructions[0],
          ScopeInstr::VarDecl {
            ty: Type {
              prim_ty: PrimType::Int(Dim::Scalar),
              array_dims: Vec::new(),
            },
            handle: ScopedHandle::fun_var(0, 0),
            init_value: ErasedExpr::LitInt(3),
          }
        )
      }
      _ => panic!("wrong type"),
    }
  }

  #[test]
  fn swizzling() {
    let mut scope = Scope::<()>::new(0);
    let foo = scope.var(lit![1, 2]);
    let foo_xy = sw!(foo, .x.y);
    let foo_xx = sw!(foo, .x.x);

    assert_eq!(
      foo_xy.erased,
      ErasedExpr::Swizzle(
        Box::new(ErasedExpr::MutVar(ScopedHandle::fun_var(0, 0))),
        Swizzle::D2(SwizzleSelector::X, SwizzleSelector::Y),
      )
    );

    assert_eq!(
      foo_xx.erased,
      ErasedExpr::Swizzle(
        Box::new(ErasedExpr::MutVar(ScopedHandle::fun_var(0, 0))),
        Swizzle::D2(SwizzleSelector::X, SwizzleSelector::X),
      )
    );
  }

  #[test]
  fn when() {
    let mut s = Scope::<Expr<V4<f32>>>::new(0);

    let x = s.var(1);
    s.when(x.eq(lit!(2)), |s| {
      let y = s.var(lit![1., 2., 3., 4.]);
      s.leave(y);
    })
    .or_else(x.eq(lit!(0)), |s| s.leave(lit![0., 0., 0., 0.]))
    .or(|_| ());

    assert_eq!(s.erased.instructions.len(), 4);

    assert_eq!(
      s.erased.instructions[0],
      ScopeInstr::VarDecl {
        ty: Type {
          prim_ty: PrimType::Int(Dim::Scalar),
          array_dims: Vec::new(),
        },
        handle: ScopedHandle::fun_var(0, 0),
        init_value: ErasedExpr::LitInt(1),
      }
    );

    // if
    let mut scope = ErasedScope::new(1);
    scope.next_var = 1;
    scope.instructions.push(ScopeInstr::VarDecl {
      ty: Type {
        prim_ty: PrimType::Float(Dim::D4),
        array_dims: Vec::new(),
      },
      handle: ScopedHandle::fun_var(1, 0),
      init_value: ErasedExpr::LitFloat4([1., 2., 3., 4.]),
    });
    scope
      .instructions
      .push(ScopeInstr::Return(ErasedReturn::Expr(
        V4::<f32>::ty(),
        ErasedExpr::MutVar(ScopedHandle::fun_var(1, 0)),
      )));

    assert_eq!(
      s.erased.instructions[1],
      ScopeInstr::If {
        condition: ErasedExpr::Eq(
          Box::new(ErasedExpr::MutVar(ScopedHandle::fun_var(0, 0))),
          Box::new(ErasedExpr::LitInt(2)),
        ),
        scope,
      }
    );

    // else if
    let mut scope = ErasedScope::new(1);
    scope
      .instructions
      .push(ScopeInstr::Return(ErasedReturn::Expr(
        V4::<f32>::ty(),
        ErasedExpr::LitFloat4([0., 0., 0., 0.]),
      )));

    assert_eq!(
      s.erased.instructions[2],
      ScopeInstr::ElseIf {
        condition: ErasedExpr::Eq(
          Box::new(ErasedExpr::MutVar(ScopedHandle::fun_var(0, 0))),
          Box::new(ErasedExpr::LitInt(0)),
        ),
        scope,
      }
    );

    // else
    assert_eq!(
      s.erased.instructions[3],
      ScopeInstr::Else {
        scope: ErasedScope::new(1)
      }
    );
  }

  #[test]
  fn for_loop() {
    let mut scope: Scope<Expr<i32>> = Scope::new(0);

    scope.loop_for(
      0,
      |a| a.lt(lit!(10)),
      |a| a + 1,
      |s, a| {
        s.leave(a);
      },
    );

    assert_eq!(scope.erased.instructions.len(), 1);

    let mut loop_scope = ErasedScope::new(1);
    loop_scope.next_var = 1;
    loop_scope.instructions.push(ScopeInstr::VarDecl {
      ty: Type {
        prim_ty: PrimType::Int(Dim::Scalar),
        array_dims: Vec::new(),
      },
      handle: ScopedHandle::fun_var(1, 0),
      init_value: ErasedExpr::LitInt(0),
    });
    loop_scope
      .instructions
      .push(ScopeInstr::Return(ErasedReturn::Expr(
        i32::ty(),
        ErasedExpr::MutVar(ScopedHandle::fun_var(1, 0)),
      )));

    assert_eq!(
      scope.erased.instructions[0],
      ScopeInstr::For {
        init_ty: i32::ty(),
        init_handle: ScopedHandle::fun_var(1, 0),
        init_expr: ErasedExpr::MutVar(ScopedHandle::fun_var(1, 0)),
        condition: ErasedExpr::Lt(
          Box::new(ErasedExpr::MutVar(ScopedHandle::fun_var(1, 0))),
          Box::new(ErasedExpr::LitInt(10)),
        ),
        post_expr: ErasedExpr::Add(
          Box::new(ErasedExpr::MutVar(ScopedHandle::fun_var(1, 0))),
          Box::new(ErasedExpr::LitInt(1)),
        ),
        scope: loop_scope,
      }
    );
  }

  #[test]
  fn while_loop() {
    let mut scope: Scope<Expr<i32>> = Scope::new(0);

    scope.loop_while(lit!(1).lt(lit!(2)), Scope::loop_continue);

    let mut loop_scope = ErasedScope::new(1);
    loop_scope.instructions.push(ScopeInstr::Continue);

    assert_eq!(scope.erased.instructions.len(), 1);
    assert_eq!(
      scope.erased.instructions[0],
      ScopeInstr::While {
        condition: ErasedExpr::Lt(
          Box::new(ErasedExpr::LitInt(1)),
          Box::new(ErasedExpr::LitInt(2)),
        ),
        scope: loop_scope,
      }
    );
  }

  #[test]
  fn vertex_id_commutative() {
    let vertex = VertexShaderEnv::new();

    let x = lit!(1);
    let _ = &vertex.vertex_id + &x;
    let _ = x + vertex.vertex_id;
  }

  #[test]
  fn array_lookup() {
    let vertex = VertexShaderEnv::new();
    let clip_dist_expr = vertex.clip_distance.at(1);

    assert_eq!(
      clip_dist_expr.erased,
      ErasedExpr::ArrayLookup {
        object: Box::new(vertex.clip_distance.erased.clone()),
        index: Box::new(ErasedExpr::LitInt(1)),
      }
    );
  }

  #[test]
  fn array_creation() {
    let _ = Expr::from([1, 2, 3]);
    let _ = Expr::from(&[1, 2, 3]);
    let two_d = Expr::from([[1, 2], [3, 4]]);

    assert_eq!(
      two_d.erased,
      ErasedExpr::Array(
        <[[i32; 2]; 2] as ToType>::ty(),
        vec![
          ErasedExpr::Array(
            <[i32; 2] as ToType>::ty(),
            vec![ErasedExpr::LitInt(1), ErasedExpr::LitInt(2)]
          ),
          ErasedExpr::Array(
            <[i32; 2] as ToType>::ty(),
            vec![ErasedExpr::LitInt(3), ErasedExpr::LitInt(4)]
          )
        ]
      )
    );
  }
}
