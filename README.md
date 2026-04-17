# EksLang
### A statically typed, ECS-oriented programming language compiled with the LLVM toolchain.

#### What is EksLang?
EksLang is not a real language to make programs in,
right now. EksLang is really for me to explore
creating a compiler, and optimizing for
a domain-specific language that works off an
entirely different paradigm than most
languages.

#### How does it work?

EksLang programs are composed of `ent`s, which
are themselves composed of `comp`s. `comp`s are 
composed of primitive types. `fun`s operate
on primitive types and user `comp`s. `sys`s 
query all existing `ent` instantiations which
are stored as a struct of arrays of their 
components. 

#### Ok, how does it work?

Fair.

### fun
`fun` is our function keyword. it defines a procedure
which can take one or more elements as 
arguments, execute any number of statements,
and return an expression.

`fun`s are defined by the following EBNF:

```
fun = "fun", ["(", type, ")"], ident, 
["(", { ident, ":", type, "," }, ")"], "{"
block, "}";
```

`fun`s have their return types in parentheses
after the keyword:

```
fun(int) factorial(n: int) {
    if n is 0 {
        return 1;
    } else {
        return n * factorial(n - 1);
    }
}
```

however, you can elide the return parens entirely if
the return type is void (although fun(void) is allowed):

```
fun hello(name: string) {
    println("hello " + name);
}
```

if there's no params, they can also be elided:

```
fun hello_world{
    println("hello world!");
}
```

