(comment) @comment
(doc_string) @comment.doc

[
  "role"
  "enum"
  "scalar"
  "model"
  "view"
  "fields"
  "actions"
  "reads"
] @keyword

[
  "extends"
  "derives"
  "where"
  "with"
  "by"
  "for"
] @keyword

(action_kind) @keyword

(type_identifier) @type
(enum_member) @constant
(field_identifier) @property
(function_identifier) @function
(parameter) @variable.parameter
(attribute "@" @attribute)
(attribute_identifier) @attribute
(named_argument name: (identifier) @property)

(call_expression function: (reference (identifier) @function .))

(string) @string
(number) @number
(boolean) @boolean
(null) @constant.builtin

[
  "=" "==" "!=" "<" ">" "<=" ">="
  "&&" "||" "!" "+" "-" "*" "/" "^"
  "?"
] @operator

(ternary_expression ":" @operator)

["(" ")" "{" "}" "[" "]"] @punctuation.bracket
["," "." ":" ";"] @punctuation.delimiter
