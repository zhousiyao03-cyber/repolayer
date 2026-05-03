export function callBenefit(userId: number) {
  return client.GetBenefit(userId);
}

export function callRedeem(benefitId: string) {
  return client.Redeem(benefitId);
}

declare const client: {
  GetBenefit(id: number): string;
  Redeem(id: string): boolean;
};
