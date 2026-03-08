---
page_classifier: Classifier
---

## Classifier

Classify this page from a City of Whittier "Claim for Damages to Persons or Property" form. Look at the content and section numbers to determine which page this is.

```js
export default z.object({
  page_type: z.enum([
    "claim_page1",
    "claim_page2",
    "unknown"
  ])
})
```

## claim_page1

Extract page 1 of a City of Whittier damage claim form. This page contains the claimant's personal information, incident details, and a general description of the injury or damage.

The form header includes a city clerk received stamp (upper right corner) with a date, and a handwritten claim/file number near the top.

Field notes:
- All dates should be YYYY-MM-DD format
- All times should be HH:MM 24-hour format
- `city_clerk_received_date` comes from the received stamp in the upper right corner
- `claim_number` is the file/claim number assigned by the city clerk, often handwritten near the top
- `claimant_date_of_birth` is YYYY-MM-DD
- `social_security_number` and `drivers_license_number` may be redacted/blacked out — return null if unreadable
- Section 5 (mail notices) may be the same as or different from the claimant address in section 2
- Section 6: if the claim is made by one person on behalf of another, extract the representative's name, relationship, and address
- Section 7: if the injured/damaged person is a minor, extract their birth date (YYYY-MM-DD)
- `place_of_occurrence` should include the full text as written — street address, cross streets, or description of the location
- `general_description` should be the full verbatim text of the claimant's description of what happened and the injury, damage, or loss sustained

Most fields are handwritten and many may be left blank.

```schema
export default z.object({
  city_clerk_received_date: z.nullable(z.string()),
  claim_number: z.nullable(z.string()),

  claimant_name: z.nullable(z.string()),
  claimant_date_of_birth: z.nullable(z.string()),

  claimant_street_address: z.nullable(z.string()),
  claimant_city: z.nullable(z.string()),
  claimant_state: z.nullable(z.string()),
  claimant_zip: z.nullable(z.string()),

  social_security_number: z.nullable(z.string()),
  drivers_license_number: z.nullable(z.string()),

  phone_number: z.nullable(z.string()),

  mail_notices_street_address: z.nullable(z.string()),
  mail_notices_city: z.nullable(z.string()),
  mail_notices_state: z.nullable(z.string()),
  mail_notices_zip: z.nullable(z.string()),

  claim_made_by_another_person: z.nullable(z.boolean()),
  representative_name: z.nullable(z.string()),
  representative_relationship: z.nullable(z.string()),
  representative_address: z.nullable(z.string()),
  representative_city: z.nullable(z.string()),
  representative_state: z.nullable(z.string()),
  representative_zip: z.nullable(z.string()),

  person_is_minor: z.nullable(z.boolean()),
  minor_birth_date: z.nullable(z.string()),

  date_of_incident: z.nullable(z.string()),
  time_of_incident: z.nullable(z.string()),

  place_of_occurrence: z.nullable(z.string()),

  general_description: z.nullable(z.string()),
})
```

## claim_page2

Extract page 2 of a City of Whittier damage claim form. This page contains sections about the basis of the claim, property conditions, bodily injury details, police/paramedic investigation info, the dollar amount claimed, and the claimant's signature/declaration.

Field notes:
- All dates should be YYYY-MM-DD format
- Section 11: whether the claim is based on an act or omission of a city employee. If so, extract the employee name and a statement of their involvement.
- Section 12: whether the claim is based on a dangerous or defective condition of public property. If so, extract the property description, date the city was notified (YYYY-MM-DD), the name of the city employee notified, and a general statement of the incident.
- Section 13: whether bodily injury is claimed. If so, extract physician/hospital names and contact info, and names/contact info of any witnesses.
- Section 14: whether the incident was investigated by police. If so, extract the report number and department/city. Also whether paramedics were called, and the report number and ambulance company name if applicable.
- `claim_dollar_amount` is the dollar value of the claim as written (e.g. "$5,000" or "unknown")
- `executed_date` is the date the form was signed (YYYY-MM-DD)
- `signature_name` is the printed or legible name from the signature area
- `signer_address` is the full address line of the claimant or agent who signed

Most fields may be left blank.

```schema
export default z.object({
  basis_is_city_employee_act: z.nullable(z.boolean()),
  city_employee_name: z.nullable(z.string()),
  city_employee_involvement_statement: z.nullable(z.string()),

  basis_is_dangerous_condition: z.nullable(z.boolean()),
  describe_public_property: z.nullable(z.string()),
  date_of_notification: z.nullable(z.string()),
  name_of_city_employee_notified: z.nullable(z.string()),
  general_statement_of_incident: z.nullable(z.string()),

  bodily_injury_claimed: z.nullable(z.boolean()),
  physician_name_and_contact: z.nullable(z.string()),
  hospital_name_and_contact: z.nullable(z.string()),
  witnesses: z.nullable(z.string()),

  incident_investigated_by_police: z.nullable(z.boolean()),
  police_report_number: z.nullable(z.string()),
  police_department_or_city: z.nullable(z.string()),
  paramedics_called: z.nullable(z.boolean()),
  paramedic_report_number: z.nullable(z.string()),
  ambulance_company_name: z.nullable(z.string()),

  claim_dollar_amount: z.nullable(z.string()),

  executed_date: z.nullable(z.string()),
  signature_name: z.nullable(z.string()),
  signer_address: z.nullable(z.string()),
})
```

## unknown

Unknown or unrecognized page from a Whittier damage claim form. Extract any legible text on the page.

```schema
export default z.object({
  raw_text: z.string()
})
```
