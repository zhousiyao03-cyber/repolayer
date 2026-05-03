package services

import "fmt"

func GetBenefit(userId int64) (string, error) {
	return fmt.Sprintf("benefit-%d", userId), nil
}

func Redeem(benefitId string) bool {
	return true
}
