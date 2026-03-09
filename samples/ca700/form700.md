---
page_classifier: Classifier
---

## Classifier

go 
```js
export default z.object({
  page_type: z.enum([
    "Cover Page",
    "schedule-a-1",
    "schedule-a-2",
    "schedule-b",
    "schedule-c",
    "schedule-d",
    "unknown"
  ])
})
```

## Cover Page

go

```schema
export default z.object({
  filed_date: z.string(),
  filter_name: z.string(),
  agency_name: z.string(),
  position: z.string(),
  jurisdiction: z.string(),
  statement_type: z.enum(["annual", "assuming_office", "candidate", "leaving-office"]),
  //attached_schedules: ,
  date_signed: z.string(),

})
```

## schedule-a-1

go

```schema

const LineItem = z.object({
  business_entity_name: z.string(),
  business_description: z.string(),
  fair_market_value: z.enum([
    "2,000-10,000",
    "10,001-100,000",
    "100,001-1,000,000",
    "+1,000,000"
  ]),
  nature_of_investment: z.enum([
    "stock",
    "partnership",
    "other"
  ])
})
export default z.object({
  line_items: z.array(LineItem)
})
```

## schedule-a-2

go 

```schema
const LineItem = z.object({
  entity_name: z.string(),
  entity_address: z.string(),
  entity_type: z.enum(["trust", "business"]),
  business_description: z.nullable(z.string()),
  business_fair_market_value: z.nullable(z.enum([
    "2,000-10,000",
    "10,001-100,000",
    "100,001-1,000,000",
    "+1,000,000"
  ])),
  business_nature_investment: z.nullable(z.enum(["Parnership", "Sole", "Other"])),
  business_position: z.nullable(z.string()),
  gross_income_received: z.enum([
    "0-499",
    "500-1,000",
    "1,001-10,000",
    "10,001-100,000",
    "+100,000"
  ]),
  // TODO #3
  // TODO #4
})
export default z.object({
  line_items: z.array(LineItem)
})
```

## schedule-b

go 

```schema
const LineItem = z.object({
  parcel_number_or_address: z.string(),
  city: z.string(),
  fair_market_value: z.enum([
    "2,000-10,000",
    "10,001-100,000",
    "100,001-1,000,000",
    "+1,000,000"
  ])
})
export default z.object({
  line_items: z.array(LineItem)
})
```

## schedule-c

go 

```schema
const LineItem = z.object({
  name_income_source: z.string(),
  address: z.string(),
  business_position: z.string(),
  gross_income_received: z.enum([
    "0-499",
    "500-1,000",
    "1,001-10,000",
    "10,001-100,000",
    "+100,000"
  ]),
})
export default z.object({
  line_items: z.array(LineItem)
})
```

## schedule-d

Parse the following PDF into its separate line items.

Keep in mind, the page layout contains a 2x3 grid of 6 "line items". The 1st line item should be the upper left, 2nd upper right, 3rd middle left, 4th middle right, 5th lower left, 6th lower right.

```
---------
| 1 | 2 |
| 3 | 4 | 
| 5 | 6 | 
---------
```

```schema
const Gift = z.object({
  date: z.string(),
  value: z.number(),
  description: z.string(),
})
const LineItem = z.object({
  source_name: z.string(),
  source_address: z.string(),
  business_activity: z.string(),
  gifts: z.array(Gift),
})
export default z.object({
  line_items: z.array(LineItem)
})
```

## unknown

Unknown or unrecognized page type.

```schema
export default z.object({
  raw_text: z.string()
})
```