/**
 * Tree-sitter grammar for the Plato schema language, for editor highlighting.
 *
 * The language's source of truth is the Chevrotain lexer and parser in plato2,
 * `packages/plato-schema/src/lexer.ts` and `parse.ts`; this grammar follows
 * them rule for rule and should change with them. Where it is looser (it does
 * not report dangling docstrings or check action kinds) it is because an
 * editor only needs the shape.
 *
 * @file Plato schema grammar
 */

/// <reference types="tree-sitter-cli/dsl" />
// @ts-check

const PREC = {
	ternary: 1,
	or: 2,
	and: 3,
	equality: 4,
	relation: 5,
	add: 6,
	multiply: 7,
	unary: 8,
	call: 9,
};

/** One or more `rule`s separated by `separator`. */
function sep1(rule, separator) {
	return seq(rule, repeat(seq(separator, rule)));
}

module.exports = grammar({
	name: "plato",

	// Docstrings document whatever follows them; the editor needs only to see
	// them, so they may appear anywhere, like comments.
	extras: ($) => [/\s/, $.comment, $.doc_string],

	word: ($) => $.identifier,

	rules: {
		document: ($) =>
			repeat(
				choice(
					$.model_declaration,
					$.view_declaration,
					$.scalar_declaration,
					$.enum_declaration,
					$.role_declaration,
				),
			),

		/* ---------------- Declarations ---------------- */

		role_declaration: ($) =>
			seq("role", field("name", alias($.identifier, $.type_identifier))),

		enum_declaration: ($) =>
			seq(
				"enum",
				field("name", alias($.identifier, $.type_identifier)),
				$.enum_body,
			),

		enum_body: ($) => seq("{", repeat1($._enum_value), "}"),

		_enum_value: ($) => choice(alias($.identifier, $.enum_member), $.string),

		scalar_declaration: ($) =>
			seq(
				"scalar",
				field("name", alias($.identifier, $.type_identifier)),
				"=",
				field("class", alias($.identifier, $.type_identifier)),
				$.arguments,
			),

		model_declaration: ($) =>
			seq(
				"model",
				field("name", alias($.identifier, $.type_identifier)),
				optional($.extends_clause),
				optional($.derives_clause),
				"{",
				repeat(
					choice($.fields_block, $.actions_block, $.reads_block, $.attribute),
				),
				"}",
			),

		extends_clause: ($) =>
			seq("extends", sep1(alias($.identifier, $.type_identifier), ",")),

		derives_clause: ($) =>
			seq(
				"derives",
				field("base", alias($.identifier, $.type_identifier)),
				optional($.where_clause),
			),

		where_clause: ($) => seq("where", "(", $._expression, ")"),

		/* ---------------- Fields ---------------- */

		fields_block: ($) => seq("fields", "{", repeat($.model_field), "}"),

		model_field: ($) =>
			seq(
				field("name", alias($.identifier, $.field_identifier)),
				field("type", choice($.inline_enum, $.field_type)),
				optional("?"),
				optional(seq("=", field("value", $._expression))),
				repeat($.attribute),
			),

		field_type: ($) =>
			seq(alias($.identifier, $.type_identifier), optional(seq("[", "]"))),

		inline_enum: ($) => seq("enum", $.enum_body),

		/* ---------------- Actions ---------------- */

		actions_block: ($) => seq("actions", "{", repeat($.action), "}"),

		action: ($) =>
			seq(
				field("kind", $.action_kind),
				field("name", alias($.identifier, $.function_identifier)),
				$.parameters,
				optional($.with_clause),
				optional($.where_clause),
				optional($.by_clause),
				optional($.code_block),
			),

		action_kind: (_) => choice("create", "update", "delete"),

		parameters: ($) =>
			seq("(", sep1(alias($.identifier, $.parameter), ","), ")"),

		with_clause: ($) =>
			seq(
				"with",
				"(",
				sep1(seq(alias($.identifier, $.field_identifier), optional("?")), ","),
				")",
			),

		by_clause: ($) => seq("by", $.principal_list),

		principal_list: ($) => prec.right(sep1($._expression, ",")),

		code_block: ($) => seq("{", repeat(choice(";", $.assignment)), "}"),

		assignment: ($) =>
			seq(
				field("target", $.assignment_target),
				"=",
				field("value", $._expression),
			),

		assignment_target: ($) => sep1($.identifier, "."),

		/* ---------------- Reads ---------------- */

		// One entry per line, each a comma-separated list of principals.
		reads_block: ($) => seq("reads", "{", repeat($.principal_list), "}"),

		/* ---------------- Views ---------------- */

		view_declaration: ($) =>
			seq(
				"view",
				optional(field("name", alias($.identifier, $.type_identifier))),
				optional(seq("for", sep1(alias($.identifier, $.type_identifier), ","))),
				$.view_body,
			),

		view_body: ($) => seq("{", repeat($.view_field), "}"),

		view_field: ($) =>
			seq(
				optional(
					seq(field("alias", alias($.identifier, $.field_identifier)), ":"),
				),
				field("field", alias($.identifier, $.field_identifier)),
				optional(choice("?", "!")),
				repeat($.attribute),
				optional($.view_body),
			),

		/* ---------------- Attributes ---------------- */

		attribute: ($) =>
			prec.right(
				seq(
					"@",
					field("name", alias($.identifier, $.attribute_identifier)),
					optional($.arguments),
				),
			),

		arguments: ($) =>
			seq(
				"(",
				optional(sep1(choice($.named_argument, $._expression), ",")),
				")",
			),

		named_argument: ($) =>
			seq(field("name", $.identifier), ":", field("value", $._expression)),

		/* ---------------- Expressions ---------------- */

		_expression: ($) =>
			choice(
				$.ternary_expression,
				$.binary_expression,
				$.unary_expression,
				$.parenthesized_expression,
				$.call_expression,
				$.reference,
				$._literal,
			),

		ternary_expression: ($) =>
			prec.right(
				PREC.ternary,
				seq(
					field("condition", $._expression),
					"?",
					field("consequence", $._expression),
					":",
					field("alternative", $._expression),
				),
			),

		binary_expression: ($) => {
			const table = [
				[PREC.or, "||"],
				[PREC.and, "&&"],
				[PREC.equality, choice("==", "!=")],
				[PREC.relation, choice("<", ">", "<=", ">=")],
				[PREC.add, choice("+", "-")],
				[PREC.multiply, choice("*", "/", "^")],
			];
			return choice(
				...table.map(([precedence, operator]) =>
					prec.left(
						/** @type {number} */ (precedence),
						seq(
							field("left", $._expression),
							field("operator", operator),
							field("right", $._expression),
						),
					),
				),
			);
		},

		unary_expression: ($) =>
			prec(
				PREC.unary,
				seq(
					field("operator", choice("!", "-")),
					field("operand", $._expression),
				),
			),

		parenthesized_expression: ($) => seq("(", $._expression, ")"),

		// A call's arguments open on its name's line in the parser; requiring
		// them to follow the name directly is the nearest a grammar gets, and
		// keeps `reads` entries that start with `(` their own.
		call_expression: ($) =>
			prec(
				PREC.call,
				seq(
					field("function", $.reference),
					alias($._call_arguments, $.arguments),
				),
			),

		_call_arguments: ($) =>
			seq(
				token.immediate("("),
				optional(sep1(choice($.named_argument, $._expression), ",")),
				")",
			),

		reference: ($) => prec.left(sep1($.identifier, ".")),

		_literal: ($) => choice($.string, $.number, $.boolean, $.null),

		/* ---------------- Tokens ---------------- */

		identifier: (_) => /[A-Za-z_$][\w$]*/,

		number: (_) => /\d+(\.\d*)?|\.\d+/,

		boolean: (_) => choice("true", "false"),

		null: (_) => "null",

		string: (_) =>
			token(
				choice(
					seq('"', repeat(choice(/[^"\\\r\n]/, /\\./)), '"'),
					seq("'", repeat(choice(/[^'\\\r\n]/, /\\./)), "'"),
				),
			),

		doc_string: (_) =>
			token(seq('"""', repeat(choice(/[^"]/, /"[^"]/, /""[^"]/)), '"""')),

		comment: (_) => token(seq("#", /[^\r\n]*/)),
	},
});
