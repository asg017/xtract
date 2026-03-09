---
page_classifier: Classifier
---

## Classifier

Classify this page from a California FPPC Form 460 (Recipient Committee Campaign Statement). Look at the schedule label in the top-left corner and the header area to determine the page type.

```js
export default z.object({
  page_type: z.enum([
    "cover-page-part-1",
    "cover-page-part-2",
    "summary-page",
    "schedule-a",
    "schedule-b-part-1",
    "schedule-b-part-2",
    "schedule-c",
    "schedule-d",
    "schedule-e",
    "schedule-f",
    "schedule-g",
    "schedule-h",
    "schedule-i",
    "unknown"
  ])
})
```

## cover-page-part-1

Extract the Cover Page (Part 1) of FPPC Form 460. This page contains the statement period, committee type, statement type, committee information, and treasurer information. Dates should be in YYYY-MM-DD format. The "Type of Recipient Committee" section has checkboxes -- pick the one that is checked. Similarly for "Type of Statement".

```schema
export default z.object({
  statement_covers_period_from: z.string().describe("YYYY-MM-DD"),
  statement_covers_period_through: z.string().describe("YYYY-MM-DD"),
  date_of_election: z.nullable(z.string()).describe("YYYY-MM-DD if applicable"),
  page_number: z.number(),
  total_pages: z.number(),
  type_of_recipient_committee: z.enum([
    "officeholder_candidate_controlled",
    "state_candidate_election",
    "recall",
    "general_purpose_sponsored",
    "general_purpose_small_contributor",
    "general_purpose_political_party_central",
    "primarily_formed_ballot_measure",
    "primarily_formed_ballot_measure_controlled",
    "primarily_formed_ballot_measure_sponsored",
    "primarily_formed_candidate_officeholder"
  ]),
  type_of_statement: z.enum([
    "preelection",
    "semi_annual",
    "termination",
    "quarterly",
    "special_odd_year_report",
    "amendment"
  ]),
  committee_name: z.string(),
  id_number: z.string(),
  committee_street_address: z.nullable(z.string()),
  committee_city: z.nullable(z.string()),
  committee_state: z.nullable(z.string()),
  committee_zip: z.nullable(z.string()),
  committee_phone: z.nullable(z.string()),
  committee_mailing_address: z.nullable(z.string()),
  committee_mailing_city: z.nullable(z.string()),
  committee_mailing_state: z.nullable(z.string()),
  committee_mailing_zip: z.nullable(z.string()),
  committee_email: z.nullable(z.string()),
  treasurer_name: z.string(),
  treasurer_mailing_address: z.nullable(z.string()),
  treasurer_city: z.nullable(z.string()),
  treasurer_state: z.nullable(z.string()),
  treasurer_zip: z.nullable(z.string()),
  treasurer_phone: z.nullable(z.string()),
  treasurer_email: z.nullable(z.string()),
  assistant_treasurer_name: z.nullable(z.string()),
  verification_executed_dates: z.array(z.string()).describe("YYYY-MM-DD for each signed date"),
})
```

## cover-page-part-2

Extract Cover Page Part 2 of FPPC Form 460. This contains Sections 5-7: officeholder/candidate info, ballot measure committee info, and primarily formed candidate/officeholder committee info. Many fields may be empty.

```schema
const RelatedCommittee = z.object({
  committee_name: z.nullable(z.string()),
  id_number: z.nullable(z.string()),
  treasurer_name: z.nullable(z.string()),
  controlled_committee: z.nullable(z.boolean()),
  committee_address: z.nullable(z.string()),
  committee_city: z.nullable(z.string()),
  committee_state: z.nullable(z.string()),
  committee_zip: z.nullable(z.string()),
  committee_phone: z.nullable(z.string()),
})

const PrimarilyFormedCandidate = z.object({
  name: z.nullable(z.string()),
  office_sought: z.nullable(z.string()),
  support_or_oppose: z.nullable(z.enum(["support", "oppose"])),
})

export default z.object({
  officeholder_candidate_name: z.nullable(z.string()),
  office_sought_or_held: z.nullable(z.string()),
  jurisdiction: z.nullable(z.string()),
  residential_business_address: z.nullable(z.string()),
  residential_city: z.nullable(z.string()),
  residential_state: z.nullable(z.string()),
  residential_zip: z.nullable(z.string()),
  ballot_measure_name: z.nullable(z.string()),
  ballot_number_or_letter: z.nullable(z.string()),
  ballot_jurisdiction: z.nullable(z.string()),
  ballot_support_or_oppose: z.nullable(z.enum(["support", "oppose"])),
  controlling_officeholder_name: z.nullable(z.string()),
  controlling_office_sought_or_held: z.nullable(z.string()),
  controlling_district_number: z.nullable(z.string()),
  related_committees: z.array(RelatedCommittee),
  primarily_formed_candidates: z.array(PrimarilyFormedCandidate),
})
```

## summary-page

Extract the Summary Page of FPPC Form 460. This is a very dense financial summary page. It has numbered lines with Column A (this period) and Column B (calendar year total / to date). Extract each numbered line carefully. Use the line number as part of the key name. All dollar amounts should be numbers (not strings). Dates in YYYY-MM-DD format.

The page has these main sections:
- Contributions Received (lines 1-5)
- Expenditures Made (lines 6-11)
- Current Cash Statement (lines 12-16)
- Loan Guarantees Received (line 17)
- Cash Equivalents and Outstanding Debts (lines 18-19)
- Calendar Year Summary for Candidates (lines 20-22)
- Expenditures Limit Summary for State Candidates (line 22 right side)

Column A = "This Period" amounts. Column B = "Calendar Year Total / To Date" amounts.

```schema
export default z.object({
  statement_covers_period_from: z.string().describe("YYYY-MM-DD"),
  statement_covers_period_through: z.string().describe("YYYY-MM-DD"),
  committee_name: z.string(),
  id_number: z.string(),

  // Contributions Received
  line_1_monetary_contributions_col_a: z.number(),
  line_1_monetary_contributions_col_b: z.number(),
  line_2_loans_received_col_a: z.number(),
  line_2_loans_received_col_b: z.number(),
  line_3_subtotal_cash_contributions_col_a: z.number(),
  line_3_subtotal_cash_contributions_col_b: z.number(),
  line_4_nonmonetary_contributions_col_a: z.number(),
  line_4_nonmonetary_contributions_col_b: z.number(),
  line_5_total_contributions_received_col_a: z.number(),
  line_5_total_contributions_received_col_b: z.number(),

  // Expenditures Made
  line_6_payments_made_col_a: z.number(),
  line_6_payments_made_col_b: z.number(),
  line_7_loans_made_col_a: z.number(),
  line_7_loans_made_col_b: z.number(),
  line_8_subtotal_cash_payments_col_a: z.number(),
  line_8_subtotal_cash_payments_col_b: z.number(),
  line_9_accrued_expenses_col_a: z.number(),
  line_9_accrued_expenses_col_b: z.number(),
  line_10_nonmonetary_adjustment_col_a: z.number(),
  line_10_nonmonetary_adjustment_col_b: z.number(),
  line_11_total_expenditures_col_a: z.number(),
  line_11_total_expenditures_col_b: z.number(),

  // Current Cash Statement
  line_12_beginning_cash_balance: z.number(),
  line_13_cash_receipts_col_a: z.number(),
  line_14_miscellaneous_increases: z.number(),
  line_15_cash_payments_col_a: z.number(),
  line_16_ending_cash_balance: z.number().describe("Line 12 + 13 + 14 - 15. If termination, must be zero."),

  // Loan Guarantees
  line_17_loan_guarantees_received_col_a: z.number(),
  line_17_loan_guarantees_received_col_b: z.number(),

  // Cash Equivalents and Outstanding Debts
  line_18_cash_equivalents: z.number(),
  line_19_outstanding_debts: z.number(),

  // Calendar Year Summary (right side of page, for candidates running in both primary and general)
  line_20_contributions_received: z.nullable(z.number()).describe("1/1 through 6/30"),
  line_20_contributions_received_to_date: z.nullable(z.number()),
  line_21_expenditures_made: z.nullable(z.number()).describe("1/1 through 6/30"),
  line_21_expenditures_made_to_date: z.nullable(z.number()),

  // Expenditures Limit Summary
  line_22_cumulative_expenditures_made: z.nullable(z.array(z.object({
    date_of_election: z.string().describe("YYYY-MM-DD"),
    total_to_date: z.number(),
  }))),
})
```

## schedule-a

Extract Schedule A - Monetary Contributions Received from FPPC Form 460. Each row is a contribution from a donor. Parse all contributor rows on the page. Dates in YYYY-MM-DD format. The contributor code is a checkbox (IND, COM, OTH, PTY, SCC) -- pick the one that is checked.

```schema
const Contribution = z.object({
  date_received: z.string().describe("YYYY-MM-DD"),
  contributor_name: z.string(),
  contributor_street_address: z.nullable(z.string()),
  contributor_city_state_zip: z.string(),
  contributor_code: z.enum(["IND", "COM", "OTH", "PTY", "SCC"]),
  contributor_occupation_or_business: z.nullable(z.string()).describe("If IND, their occupation and employer. If self-employed, the business name."),
  amount_received_this_period: z.number(),
  cumulative_to_date_calendar_year: z.number(),
  per_election_to_date: z.nullable(z.number()),
})
export default z.object({
  line_items: z.array(Contribution),
  subtotal: z.nullable(z.number()),
})
```

## schedule-b-part-1

Extract Schedule B Part 1 - Loans Received from FPPC Form 460. This page has loan detail rows at the top and a Schedule B Summary at the bottom. Dates in YYYY-MM-DD format. For each loan row, columns (a) through (g) map to the fields below. The "paid or forgiven" field is a checkbox. The summary has 3 numbered lines.

```schema
const LoanReceived = z.object({
  lender_name: z.string(),
  lender_street_address: z.nullable(z.string()),
  lender_city_state_zip: z.string(),
  lender_code: z.nullable(z.enum(["IND", "COM", "OTH", "PTY", "SCC"])),
  lender_occupation_or_business: z.nullable(z.string()),
  col_a_outstanding_balance_beginning: z.number(),
  col_b_amount_received_this_period: z.number(),
  col_c_amount_paid_or_forgiven: z.number(),
  col_c_paid_or_forgiven: z.nullable(z.enum(["paid", "forgiven"])),
  col_d_outstanding_balance_close: z.number(),
  col_e_interest_paid_this_period: z.number(),
  col_e_interest_rate: z.nullable(z.string()),
  col_f_original_amount_of_loan: z.number(),
  col_g_cumulative_contributions_to_date: z.number(),
  date_due: z.nullable(z.string()).describe("YYYY-MM-DD"),
  date_incurred: z.nullable(z.string()).describe("YYYY-MM-DD"),
})
export default z.object({
  line_items: z.array(LoanReceived),
  summary_line_1_loans_received_this_period: z.number(),
  summary_line_2_loans_paid_or_forgiven: z.number(),
  summary_line_3_net_change: z.number().describe("May be negative"),
})
```

## schedule-b-part-2

Extract Schedule B Part 2 - Loan Guarantors from FPPC Form 460. Each row is a guarantor for a loan. Dates in YYYY-MM-DD format.

```schema
const LoanGuarantor = z.object({
  guarantor_name: z.string(),
  guarantor_street_address: z.nullable(z.string()),
  guarantor_city_state_zip: z.string(),
  guarantor_code: z.nullable(z.enum(["IND", "COM", "OTH", "PTY", "SCC"])),
  guarantor_occupation_or_business: z.nullable(z.string()),
  loan_lender: z.string(),
  loan_date: z.nullable(z.string()).describe("YYYY-MM-DD"),
  amount_guaranteed_this_period: z.number(),
  cumulative_to_date: z.number(),
  calendar_date: z.nullable(z.string()).describe("YYYY-MM-DD"),
  per_election: z.nullable(z.number()),
  balance_outstanding_to_date: z.number(),
})
export default z.object({
  line_items: z.array(LoanGuarantor),
  subtotal: z.nullable(z.number()),
})
```

## schedule-c

Extract Schedule C - Nonmonetary Contributions Received from FPPC Form 460. Each row is an in-kind contribution (goods/services, not cash). Parse all rows. Dates in YYYY-MM-DD format. Also extract the Schedule C Summary lines at the bottom.

```schema
const NonmonetaryContribution = z.object({
  date_received: z.string().describe("YYYY-MM-DD"),
  contributor_name: z.string(),
  contributor_street_address: z.nullable(z.string()),
  contributor_city_state_zip: z.string(),
  contributor_code: z.enum(["IND", "COM", "OTH", "PTY", "SCC"]),
  contributor_occupation_or_business: z.nullable(z.string()),
  description_of_goods_or_services: z.string(),
  amount_fair_market_value: z.number(),
  cumulative_to_date_calendar_year: z.number(),
  per_election_to_date: z.nullable(z.number()),
})
export default z.object({
  line_items: z.array(NonmonetaryContribution),
  summary_line_1_itemized_nonmonetary: z.number(),
  summary_line_2_unitemized_nonmonetary: z.number(),
  summary_line_3_total_nonmonetary: z.number(),
})
```

## schedule-d

Extract Schedule D - Summary of Expenditures Supporting/Opposing Other Candidates, Measures, and Committees from FPPC Form 460. Each row is an expenditure made to support or oppose another candidate/measure/committee. The "type of payment" is a checkbox (Monetary Contribution, Nonmonetary Contribution, Independent Expenditure). Dates in YYYY-MM-DD format. Also extract the 3 summary lines at bottom.

```schema
const Expenditure = z.object({
  date: z.string().describe("YYYY-MM-DD"),
  candidate_measure_or_committee_name: z.string(),
  office_district_or_jurisdiction: z.nullable(z.string()),
  support_or_oppose: z.enum(["support", "oppose"]),
  type_of_payment: z.enum(["monetary_contribution", "nonmonetary_contribution", "independent_expenditure"]),
  description: z.nullable(z.string()),
  amount_this_period: z.number(),
  cumulative_to_date_calendar_year: z.number(),
  per_election_to_date: z.nullable(z.number()),
})
export default z.object({
  line_items: z.array(Expenditure),
  summary_line_1_itemized: z.number(),
  summary_line_2_unitemized: z.number(),
  summary_line_3_total: z.number(),
})
```

## schedule-e

Extract Schedule E - Payments Made from FPPC Form 460. Each row is a payment/expenditure. The "CODE" column uses standardized 3-letter expense codes (CMP, CNS, CTB, CVC, FIL, FND, IND, LEG, LIT, MBR, MTG, OFC, PET, PHO, POL, POS, PRO, PRT, RAD, RFD, SAL, TEL, TRC, TRS, TSF, VOT, WEB). If a code is used, the "description" column may be blank. If no code, the payee described the payment in the description. Parse all rows on the page.

```schema
const Payment = z.object({
  payee_name: z.string(),
  payee_street_address: z.nullable(z.string()),
  payee_city_state_zip: z.string(),
  expense_code: z.nullable(z.string()).describe("3-letter code like FND, LIT, etc. Null if described instead."),
  description_of_payment: z.nullable(z.string()).describe("Free text description. Null if code used instead."),
  amount_paid: z.number(),
})
export default z.object({
  line_items: z.array(Payment),
  subtotal: z.nullable(z.number()),
})
```

## schedule-f

Extract Schedule F - Accrued Expenses (Unpaid Bills) from FPPC Form 460. This tracks bills the committee has received but not yet paid. Uses the same 3-letter expense codes as Schedule E. Also extract the Schedule F Summary at the bottom. Dates in YYYY-MM-DD format.

```schema
const AccruedExpense = z.object({
  payee_name: z.string(),
  payee_street_address: z.nullable(z.string()),
  payee_city_state_zip: z.string(),
  expense_code_or_description: z.string(),
  col_a_outstanding_balance_beginning: z.number(),
  col_b_amount_incurred_this_period: z.number(),
  col_c_amount_paid_this_period: z.number(),
  col_d_outstanding_balance_close: z.number(),
})
export default z.object({
  line_items: z.array(AccruedExpense),
  summary_line_1_incurred_totals: z.number(),
  summary_line_2_paid_totals: z.number(),
  summary_line_3_net_change: z.number(),
})
```

## schedule-g

Extract Schedule G - Payments Made by an Agent or Independent Contractor from FPPC Form 460. This lists payments made by agents/contractors on behalf of the committee. Uses the same 3-letter expense codes as Schedule E.

```schema
const AgentPayment = z.object({
  agent_or_contractor_name: z.string(),
  payee_name: z.string(),
  payee_street_address: z.nullable(z.string()),
  payee_city_state_zip: z.nullable(z.string()),
  expense_code: z.nullable(z.string()),
  description_of_payment: z.nullable(z.string()),
  amount_paid: z.number(),
})
export default z.object({
  line_items: z.array(AgentPayment),
  total: z.nullable(z.number()),
})
```

## schedule-h

Extract Schedule H - Loans Made to Others from FPPC Form 460. Each row is a loan the committee made to another entity. Columns (a)-(g) mirror Schedule B Part 1 but from the lender's perspective. Dates in YYYY-MM-DD format.

```schema
const LoanMade = z.object({
  recipient_name: z.string(),
  recipient_street_address: z.nullable(z.string()),
  recipient_city_state_zip: z.string(),
  recipient_occupation_or_business: z.nullable(z.string()),
  col_a_outstanding_balance_beginning: z.number(),
  col_b_amount_loaned_this_period: z.number(),
  col_c_repayment_or_forgiveness_this_period: z.number(),
  col_c_paid_or_forgiven: z.nullable(z.enum(["paid", "forgiven"])),
  col_d_outstanding_balance_close: z.number(),
  col_e_interest_received: z.number(),
  col_e_interest_rate: z.nullable(z.string()),
  col_f_original_amount_of_loan: z.number(),
  col_g_cumulative_loans_to_date: z.number(),
  date_due: z.nullable(z.string()).describe("YYYY-MM-DD"),
  date_incurred: z.nullable(z.string()).describe("YYYY-MM-DD"),
})
export default z.object({
  line_items: z.array(LoanMade),
})
```

## schedule-i

Extract Schedule I - Miscellaneous Increases to Cash from FPPC Form 460. This captures non-contribution receipts (e.g., interest, refunds). Also extract the Schedule I Summary at the bottom. Dates in YYYY-MM-DD format.

```schema
const MiscIncrease = z.object({
  date_received: z.string().describe("YYYY-MM-DD"),
  source_name: z.string(),
  source_address: z.nullable(z.string()),
  description_of_receipt: z.string(),
  amount_increase_to_cash: z.number(),
})
export default z.object({
  line_items: z.array(MiscIncrease),
  summary_line_1_itemized_increases: z.number(),
  summary_line_2_unitemized_increases: z.number(),
  summary_line_3_interest_on_loans: z.number(),
  summary_line_4_total_miscellaneous: z.number(),
})
```

## unknown

Unknown or unrecognized page type from a Form 460.

```schema
export default z.object({
  raw_text: z.string()
})
```
