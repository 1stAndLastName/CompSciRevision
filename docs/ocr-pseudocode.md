# OCR pseudocode conventions (H446)

A summary of appendix 5d of the OCR A Level Computer Science specification (H446, version 3.0), for writing notes, flashcards and quiz questions in the style students will see in the exam. The examples are our own. The full appendix is in `sources/ocr-h446-spec.pdf` (pages 32 to 43).

**Students do not have to memorise this syntax.** In the exam they may answer in any pseudocode a competent programmer could follow. Our content should still *use* OCR's style, so students get used to reading it.

## Writing it in our content

- Put pseudocode in fenced code blocks (```` ``` ````) so it uses the mono font.
- Indent blocks by 4 spaces.
- Use straight quotes (`"text"`). The PDF shows curly quotes, but those are just typesetting.
- Keywords are lowercase (`if`, `endwhile`), except the logical operators `AND`, `OR`, `NOT` and the arithmetic operators `MOD`, `DIV`, which are uppercase.

## Variables, input and output

```
score = 0                      // declared by first assignment; type comes from the value
global maxLives = 3            // main-program variable made global
lives = int("3")               // casting: int(), float(), str()
name = input("Enter your name")
print("Hello " + name)
```

Variables declared inside a function or procedure are local to it.

## Operators

| Kind | Operators |
| --- | --- |
| Comparison | `==` `!=` `<` `<=` `>` `>=` |
| Arithmetic | `+` `-` `*` `/` `^` (power), `MOD` (remainder), `DIV` (whole-number quotient) |
| Logical | `AND` `OR` `NOT` |

For example, `17 MOD 5` is 2, `17 DIV 5` is 3 and `2^5` is 32.

## Selection

```
if mark >= 70 then
    print("A")
elseif mark >= 60 then
    print("B")
else
    print("Keep going")
endif

switch day:
    case "Sat":
        print("Weekend")
    case "Sun":
        print("Weekend")
    default:
        print("Weekday")
endswitch
```

## Iteration

```
for i = 1 to 10          // both ends included: runs 10 times
    print(i)
next i

while guess != secret
    guess = input("Guess again")
endwhile

do                       // body always runs at least once
    pin = input("PIN")
until pin == "1234"
```

## Strings

```
word = "Revision"
print(word.length)             // 8
print(word.substring(0, 3))    // "Rev": start position, number of characters
```

Positions count from 0. The appendix spells the method both `subString` and `substring`; treat them as the same.

## Subroutines

```
function square(n)
    return n * n
endfunction

procedure greet(name)
    print("Hi " + name)
endprocedure

area = square(4)
greet("Sam")
```

Parameters are passed **by value** unless stated otherwise. When it matters, the question marks each one:

```
procedure swap(a:byRef, b:byRef)
```

## Arrays

```
array scores[5]          // indexes 0 to 4
scores[0] = 12
array grid[3,3]          // 2D array
grid[1,2] = "X"
```

Arrays are 0-based and declared with `array`.

## Files

```
f = openRead("data.txt")
while NOT f.endOfFile()
    print(f.readLine())
endwhile
f.close()

f = openWrite("out.txt")     // overwrites any existing contents
f.writeLine("Done")
f.close()
```

## Comments

`//` starts a comment.

## Object-oriented pseudocode

```
class Account
    private balance
    public procedure new(start)       // constructor is always a procedure called new
        balance = start
    endprocedure
    public function getBalance()
        return balance
    endfunction
endclass

class Savings inherits Account
    private rate
    public procedure new(start, givenRate)
        super.new(start)               // call the superclass constructor
        rate = givenRate
    endprocedure
endclass

acc = new Savings(100, 0.02)
print(acc.getBalance())
```

- Attributes and methods are **public** unless marked `private`. The keywords only appear when access matters to the question.
- All methods are instance methods, called as `object.method()`. Students do not need static methods.
- `super.methodName()` calls the superclass version of a method.

## Other parts of appendix 5d

These are not pseudocode, but they set the notation for other spec points.

- **Little Man Computer (1.2.4):** questions use `ADD SUB STA LDA BRA BRZ BRP INP OUT HLT DAT`. Students may also write `STO`, `LOAD`, `BR`, `BZ`, `BP`, `IN`/`INPUT` and `COB`/`END`.
- **Boolean algebra (1.4.3):** questions use `∧` (AND), `∨` (OR), `¬` (NOT), `⊻` (XOR) and `≡` (equivalence), and truth tables use **T and F**. Students may also write `AND`/`.`, `OR`/`+`, `NOT`/`~`/an overbar, `XOR`/`⊕` and `↔`.
- **SQL (1.3.2):** `SELECT` (including nested), `FROM`, `WHERE`, `LIKE`, `AND`, `OR`, `DELETE`, `INSERT`, `DROP`, `JOIN` (inner join only), and the wildcards `*` and `%`.
- **HTML, CSS and JavaScript (1.3.4):** a fixed list of HTML tags (`html`, `head`, `title`, `body`, `link`, `h1` to `h3`, `img`, `a`, `div`, `form`, text and submit `input`, `p`, `li`, `ol`, `ul`, `script`) and CSS properties (colours, borders, `font-family`, `font-size`, `height`, `width`), used inline, by element, class or id. JavaScript covers the same structures as the pseudocode, with output through `innerHTML`, `document.write` and `alert`, and no OOP or file handling.
